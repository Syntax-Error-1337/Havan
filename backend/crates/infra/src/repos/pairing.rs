use crate::pg::Pg;
use domain::pairing::{
    ClaimPlan, DeviceRecord, PairingRepo, ProductRecord, TicketClaims, TicketRow,
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

        tx.commit().await.map_err(db)?;
        Ok((device_id, plan.points.len()))
    }
}
