use domain::ota::{Campaign, CampaignStatus, Firmware, JobStatus, OtaJob, OtaRepo};
use kernel::{AppError, TenantCtx};
use sqlx::Transaction;
use sqlx::postgres::Postgres;
use uuid::Uuid;

use crate::pg::Pg;

#[derive(Clone)]
pub struct PgOta {
    pg: Pg,
}

impl PgOta {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    async fn tx(&self, vendor_id: Uuid) -> Result<Transaction<'static, Postgres>, AppError> {
        self.pg
            .scoped(&TenantCtx {
                vendor_id: Some(vendor_id),
                actor: kernel::Actor::ApiKey {
                    vendor_id,
                    key_id: Uuid::nil(),
                },
                request_id: "ota".into(),
            })
            .await
    }

    /// The dispatcher has no vendor: it walks every tenant's queue.
    async fn system_tx(&self) -> Result<Transaction<'static, Postgres>, AppError> {
        self.pg.scoped(&TenantCtx::system("ota-dispatch")).await
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

type CampaignRow = (
    Uuid,
    Uuid,
    Uuid,
    Uuid,
    i32,
    String,
    time::OffsetDateTime,
    Option<time::OffsetDateTime>,
);

fn map_campaign(row: CampaignRow) -> Result<Campaign, AppError> {
    let (id, vendor_id, product_id, firmware_id, rollout_pct, status, created_at, started_at) = row;
    let status = match status.as_str() {
        "draft" => CampaignStatus::Draft,
        "running" => CampaignStatus::Running,
        "halted" => CampaignStatus::Halted,
        "completed" => CampaignStatus::Completed,
        other => {
            return Err(AppError::Internal(format!(
                "unknown campaign status {other}"
            )));
        }
    };
    Ok(Campaign {
        id,
        vendor_id,
        product_id,
        firmware_id,
        rollout_pct,
        status,
        created_at,
        started_at,
    })
}

impl OtaRepo for PgOta {
    async fn add_firmware(
        &self,
        vendor_id: Uuid,
        product_id: Uuid,
        firmware: &Firmware,
    ) -> Result<(), AppError> {
        let mut tx = self.tx(vendor_id).await?;

        sqlx::query(
            r#"INSERT INTO firmware
                 (id, product_id, version, size, sha256, ed25519_sig, object_key, status)
               VALUES ($1, $2, $3, $4, $5, $6, $7, 'signed')"#,
        )
        .bind(firmware.id)
        .bind(product_id)
        .bind(&firmware.version)
        .bind(firmware.size)
        .bind(&firmware.sha256)
        .bind(&firmware.ed25519_sig)
        .bind(&firmware.object_key)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if e.as_database_error()
                .is_some_and(|d| d.code().as_deref() == Some("23505"))
            {
                AppError::code(
                    kernel::ErrorCode::Conflict,
                    "that version already exists for this product",
                )
            } else {
                db(e)
            }
        })?;

        tx.commit().await.map_err(db)
    }

    async fn firmware_for_product(
        &self,
        vendor_id: Uuid,
        product_id: Uuid,
    ) -> Result<Vec<Firmware>, AppError> {
        let mut tx = self.tx(vendor_id).await?;
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                String,
                i64,
                String,
                String,
                String,
                time::OffsetDateTime,
            ),
        >(
            r#"SELECT id, product_id, version, size, sha256, ed25519_sig, object_key, created_at
                     FROM firmware WHERE product_id = $1 ORDER BY created_at DESC"#,
        )
        .bind(product_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .map(
                |(id, product_id, version, size, sha256, ed25519_sig, object_key, created_at)| {
                    Firmware {
                        id,
                        product_id,
                        version,
                        size,
                        sha256,
                        ed25519_sig,
                        object_key,
                        created_at,
                    }
                },
            )
            .collect())
    }

    async fn create_campaign(&self, vendor_id: Uuid, campaign: &Campaign) -> Result<(), AppError> {
        let mut tx = self.tx(vendor_id).await?;

        // A campaign for a firmware of another product would push an image the
        // device cannot run. The database is the last place that can refuse.
        let matches: Option<bool> =
            sqlx::query_scalar("SELECT f.product_id = $1 FROM firmware f WHERE f.id = $2")
                .bind(campaign.product_id)
                .bind(campaign.firmware_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;

        match matches {
            None => return Err(AppError::not_found("firmware")),
            Some(false) => {
                return Err(AppError::validation(
                    "that firmware belongs to a different product",
                ));
            }
            Some(true) => {}
        }

        sqlx::query(
            r#"INSERT INTO ota_campaigns
                 (id, vendor_id, product_id, firmware_id, rollout_pct, status)
               VALUES ($1, $2, $3, $4, $5, 'draft')"#,
        )
        .bind(campaign.id)
        .bind(vendor_id)
        .bind(campaign.product_id)
        .bind(campaign.firmware_id)
        .bind(campaign.rollout_pct)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)
    }

    async fn campaign(&self, vendor_id: Uuid, id: Uuid) -> Result<Option<Campaign>, AppError> {
        let mut tx = self.tx(vendor_id).await?;
        let row = sqlx::query_as::<_, CampaignRow>(
            r#"SELECT id, vendor_id, product_id, firmware_id, rollout_pct, status, created_at, started_at
                 FROM ota_campaigns WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        row.map(map_campaign).transpose()
    }

    async fn start_campaign(&self, vendor_id: Uuid, id: Uuid) -> Result<u64, AppError> {
        let mut tx = self.tx(vendor_id).await?;

        // Starting twice must not double-queue: the status transition is the
        // gate, and it happens in the same statement as the read.
        let row = sqlx::query_as::<_, CampaignRow>(
            r#"UPDATE ota_campaigns
                  SET status = 'running', started_at = now()
                WHERE id = $1 AND status = 'draft'
            RETURNING id, vendor_id, product_id, firmware_id, rollout_pct, status, created_at, started_at"#,
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        let Some(row) = row else {
            return Err(AppError::code(
                kernel::ErrorCode::Conflict,
                "only a draft campaign can be started",
            ));
        };

        let version: String = sqlx::query_scalar("SELECT version FROM firmware WHERE id = $1")
            .bind(row.3)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;

        // The wave is decided here because this is where the device list lives,
        // but the *rule* is the pure one in `domain::ota`, so it can be argued
        // about without a database.
        let devices: Vec<(Uuid, Option<String>)> = sqlx::query_as(
            "SELECT id, fw_version FROM devices WHERE product_id = $1 AND state = 'claimed'",
        )
        .bind(row.2)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let mut queued = 0u64;
        for (device_id, current) in devices {
            if !domain::ota::in_rollout(id, device_id, row.4) {
                continue;
            }

            // Offer only if it is an upgrade: re-flashing the same version is a
            // reboot nobody asked for.
            if current.as_deref() == Some(version.as_str()) {
                continue;
            }

            let inserted = sqlx::query(
                r#"INSERT INTO ota_jobs (campaign_id, device_id, state)
                   VALUES ($1, $2, 'pending')
                   ON CONFLICT (campaign_id, device_id) DO NOTHING"#,
            )
            .bind(id)
            .bind(device_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();

            queued += inserted;
        }

        tx.commit().await.map_err(db)?;
        Ok(queued)
    }

    async fn claim_jobs(&self, limit: i64) -> Result<Vec<OtaJob>, AppError> {
        let mut tx = self.system_tx().await?;

        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                Option<Uuid>,
                Uuid,
                String,
                String,
                String,
                i64,
                String,
            ),
        >(
            r#"WITH claimed AS (
                   UPDATE ota_jobs j
                      SET state = 'sent', updated_at = now(), attempts = j.attempts + 1
                    WHERE (j.campaign_id, j.device_id) IN (
                        SELECT campaign_id, device_id FROM ota_jobs
                         WHERE state = 'pending'
                         ORDER BY updated_at
                         LIMIT $1
                         FOR UPDATE SKIP LOCKED
                    )
                RETURNING j.campaign_id, j.device_id
               )
               SELECT c.campaign_id, o.vendor_id, d.home_id, c.device_id,
                      f.version, f.sha256, f.ed25519_sig, f.size, f.object_key
                 FROM claimed c
                 JOIN ota_campaigns o ON o.id = c.campaign_id
                 JOIN firmware f ON f.id = o.firmware_id
                 JOIN devices d ON d.id = c.device_id"#,
        )
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    campaign_id,
                    vendor_id,
                    home_id,
                    device_id,
                    version,
                    sha256,
                    sig,
                    size,
                    object_key,
                )| {
                    OtaJob {
                        campaign_id,
                        vendor_id,
                        home_id,
                        device_id,
                        version,
                        sha256,
                        sig,
                        size,
                        object_key,
                    }
                },
            )
            .collect())
    }

    async fn mark_job(
        &self,
        campaign_id: Uuid,
        device_id: Uuid,
        status: JobStatus,
    ) -> Result<(), AppError> {
        let mut tx = self.system_tx().await?;
        sqlx::query(
            "UPDATE ota_jobs SET state = $3, updated_at = now() WHERE campaign_id = $1 AND device_id = $2",
        )
        .bind(campaign_id)
        .bind(device_id)
        .bind(status.as_str())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }
}
