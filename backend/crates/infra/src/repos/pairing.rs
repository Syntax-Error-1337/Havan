use crate::pg::Pg;
use domain::pairing::{
    ClaimPlan, DeviceRecord, ExpiringCertificate, PairingRepo, ProductRecord, SerialTake,
    TicketClaims, TicketRow,
};
use kernel::{AppError, TenantCtx};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

/// Pairing runs with the **system** role, and that is deliberate.
///
/// Claim is authenticated by a ticket plus a device secret, not by a user
/// session, so there is no tenant and no home member to scope by. Every other
/// slice is scoped by the caller; this one narrows the exception to the five
/// queries below, all keyed by a value the caller proved it knows.
#[derive(Clone)]
pub struct PgPairing {
    pg: Pg,
}

impl PgPairing {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    async fn tx(&self) -> Result<Transaction<'static, Postgres>, AppError> {
        self.pg.scoped(&TenantCtx::system("provision-claim")).await
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

impl PairingRepo for PgPairing {
    async fn take_serial(&self, product_id: Uuid, serial: &str) -> Result<SerialTake, AppError> {
        let mut tx = self.tx().await?;

        // One statement, so two devices presenting the same serial cannot both
        // win: the row is only taken if nobody took it first.
        // No device_id here: the device row does not exist yet, and inventing an
        // id would leave a pointer to nothing. `claim` fills it in once the device
        // is actually written, in the same transaction as the insert.
        let taken: Option<(Uuid,)> = sqlx::query_as(
            r#"UPDATE product_serials
                  SET claimed_at = now()
                WHERE product_id = $1 AND serial = $2 AND claimed_at IS NULL
            RETURNING product_id"#,
        )
        .bind(product_id)
        .bind(serial)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        if taken.is_some() {
            tx.commit().await.map_err(db)?;
            return Ok(SerialTake::Taken);
        }

        let exists: Option<(i32,)> =
            sqlx::query_as("SELECT 1 FROM product_serials WHERE product_id = $1 AND serial = $2")
                .bind(product_id)
                .bind(serial)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(if exists.is_some() {
            SerialTake::AlreadyClaimed
        } else {
            SerialTake::Unknown
        })
    }

    async fn mint_ticket(&self, claims: &TicketClaims) -> Result<(), AppError> {
        let mut tx = self.tx().await?;
        sqlx::query(
            r#"INSERT INTO pairing_tickets (id, home_id, user_id, jti, nonce_hash, expires_at)
               VALUES ($1, $2, $3, $4, $5, to_timestamp($6))"#,
        )
        .bind(kernel::new_id())
        .bind(claims.home_id)
        .bind(claims.user_id)
        .bind(claims.jti)
        .bind(None::<String>)
        .bind(claims.exp as f64)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn ticket(&self, jti: Uuid) -> Result<Option<TicketRow>, AppError> {
        let mut tx = self.tx().await?;
        let row = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                Uuid,
                time::OffsetDateTime,
                Option<time::OffsetDateTime>,
                Option<Uuid>,
            ),
        >(
            r#"SELECT jti, home_id, user_id, expires_at, consumed_at, device_id
               FROM pairing_tickets WHERE jti = $1"#,
        )
        .bind(jti)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(row.map(
            |(jti, home_id, user_id, expires_at, consumed_at, device_id)| TicketRow {
                jti,
                home_id,
                user_id,
                expires_at,
                consumed_at,
                device_id,
            },
        ))
    }

    async fn product_by_key(&self, product_key: &str) -> Result<Option<ProductRecord>, AppError> {
        let mut tx = self.tx().await?;
        let row = sqlx::query_as::<_, (Uuid, Uuid, serde_json::Value)>(
            r#"SELECT id, vendor_id, thing_model FROM products
               WHERE product_key = $1 AND status = 'published'"#,
        )
        .bind(product_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        let Some((id, vendor_id, model)) = row else {
            return Ok(None);
        };
        let thing_model: protocol::thing_model::ThingModel = serde_json::from_value(model)
            .map_err(|e| AppError::Internal(format!("stored thing model is invalid: {e}")))?;

        Ok(Some(ProductRecord {
            id,
            vendor_id,
            thing_model,
        }))
    }

    async fn device_by_serial(
        &self,
        product_id: Uuid,
        serial: &str,
    ) -> Result<Option<DeviceRecord>, AppError> {
        let mut tx = self.tx().await?;
        let row = sqlx::query_as::<_, (Uuid, String, String, Option<Uuid>, Option<String>)>(
            r#"SELECT id, secret_hash, state, home_id, name FROM devices
               WHERE product_id = $1 AND serial = $2"#,
        )
        .bind(product_id)
        .bind(serial)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(
            row.map(|(id, secret_hash, state, home_id, name)| DeviceRecord {
                id,
                secret_hash,
                state,
                home_id,
                name,
            }),
        )
    }

    async fn store_certificate(
        &self,
        device_id: Uuid,
        cert_serial: &str,
        not_after: time::OffsetDateTime,
    ) -> Result<(), AppError> {
        let mut tx = self.tx().await?;
        sqlx::query("UPDATE devices SET cert_serial = $2, cert_not_after = $3 WHERE id = $1")
            .bind(device_id)
            .bind(cert_serial)
            .bind(not_after)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    async fn expiring_certificates(&self, days: i64) -> Result<Vec<ExpiringCertificate>, AppError> {
        // System scope: an expiry sweep crosses vendors by definition, and it carries no
        // tenant of its own.
        let mut tx = self.pg.scoped(&TenantCtx::system("cert-expiry")).await?;
        let rows: Vec<(
            Uuid,
            Uuid,
            Option<Uuid>,
            String,
            String,
            time::OffsetDateTime,
        )> = sqlx::query_as(
            r#"SELECT id, vendor_id, home_id, name, serial, cert_not_after
                     FROM devices
                    WHERE cert_not_after IS NOT NULL
                      AND cert_not_after < now() + make_interval(days => $1)
                      -- A device that was released or reset has no one to warn.
                      AND state = 'claimed'
                    ORDER BY cert_not_after"#,
        )
        .bind(days as i32)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        let now = kernel::now();
        Ok(rows
            .into_iter()
            .map(
                |(device_id, vendor_id, home_id, name, serial, not_after)| ExpiringCertificate {
                    device_id,
                    vendor_id,
                    home_id,
                    name,
                    serial,
                    not_after,
                    days_left: (not_after - now).whole_days() as i64,
                },
            )
            .collect())
    }

    async fn claim(&self, plan: ClaimPlan) -> Result<(Uuid, usize), AppError> {
        let mut tx = self.tx().await?;

        // Burn the ticket first, conditionally: two concurrent claims with the
        // same ticket cannot both win, because only one UPDATE sees
        // `consumed_at IS NULL`.
        let burned = sqlx::query(
            "UPDATE pairing_tickets SET consumed_at = now() WHERE jti = $1 AND consumed_at IS NULL",
        )
        .bind(plan.jti)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();

        if burned == 0 {
            return Err(AppError::code(
                kernel::ErrorCode::Conflict,
                "ticket_used: this ticket has already paired a device",
            ));
        }

        // The device either exists (manufactured, secret already known) or is
        // born here on trust-on-first-use for a product without a manifest.
        let device_id: Uuid = sqlx::query_scalar(
            r#"INSERT INTO devices
                 (id, vendor_id, product_id, product_key, category, home_id, name, serial,
                  secret_hash, state, claimed_at, fw_version, online)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'claimed', now(), $10, false)
               ON CONFLICT (product_id, serial) DO UPDATE
                 SET home_id = EXCLUDED.home_id,
                     state = 'claimed',
                     claimed_at = now(),
                     fw_version = COALESCE(EXCLUDED.fw_version, devices.fw_version)
               RETURNING id"#,
        )
        .bind(kernel::new_id())
        .bind(plan.vendor_id)
        .bind(plan.product_id)
        .bind(&plan.product_key)
        .bind(&plan.category)
        .bind(plan.home_id)
        .bind(&plan.name)
        .bind(&plan.serial)
        .bind(&plan.secret_hash)
        .bind(&plan.fw)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;

        // Points are derived from the product, never from what the device says.
        sqlx::query("DELETE FROM device_points WHERE device_id = $1")
            .bind(device_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;

        for point in &plan.points {
            sqlx::query(
                r#"INSERT INTO device_points
                   (id, device_id, vendor_id, capability, slot, code, dp, kind, writable,
                    unit, min_val, max_val, scale, enum_map, history, updated_at)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15, now())"#,
            )
            .bind(kernel::new_id())
            .bind(device_id)
            .bind(plan.vendor_id)
            .bind(&point.capability)
            .bind(&point.slot)
            .bind(&point.code)
            .bind(&point.dp)
            .bind(point.kind.as_str())
            .bind(point.writable)
            .bind(&point.unit)
            .bind(point.min)
            .bind(point.max)
            .bind(point.scale)
            .bind(point.enum_map.as_ref().map(|m| serde_json::json!(m)))
            .bind(point.history.as_str())
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }

        sqlx::query("UPDATE pairing_tickets SET device_id = $2 WHERE jti = $1")
            .bind(plan.jti)
            .bind(device_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;

        // Point the manifest row at the device that now owns it. Harmless when
        // the product has no manifest: the UPDATE matches nothing.
        sqlx::query(
            r#"UPDATE product_serials SET device_id = $3
                WHERE product_id = $1 AND serial = $2 AND device_id IS NULL"#,
        )
        .bind(plan.product_id)
        .bind(&plan.serial)
        .bind(device_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;
        Ok((device_id, plan.points.len()))
    }
}
