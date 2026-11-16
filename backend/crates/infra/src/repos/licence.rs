use domain::licence::{ActivationRow, LicenceRepo, LicenceSummary, NewGrant};
use kernel::{AppError, TenantCtx};
use uuid::Uuid;

use crate::pg::Pg;

/// The vendor-scoped reads and the operator grant live in one repo because they share a
/// table and a definition of "balance"; they do not share a context. Reads run as the
/// vendor (row-level security does the scoping), the grant runs as `haven_system` because
/// an operator acts on a vendor it is not.
#[derive(Clone)]
pub struct PgLicence {
    pg: Pg,
}

impl PgLicence {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    async fn vendor_tx(
        &self,
        vendor_id: Uuid,
    ) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, AppError> {
        self.pg
            .scoped(&TenantCtx {
                vendor_id: Some(vendor_id),
                actor: kernel::Actor::User {
                    user_id: Uuid::nil(),
                    vendor_id: Some(vendor_id),
                },
                request_id: "licences".into(),
            })
            .await
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

impl LicenceRepo for PgLicence {
    async fn summary(&self, vendor_id: Uuid) -> Result<LicenceSummary, AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;

        // Every figure is a cast of a subquery over rows in two tables; the casts are
        // explicit because `sum(bigint)` is `numeric` in Postgres and would decode as a
        // decimal we then have to parse.
        let row: (i64, i64, i64, i64, i64, Option<i32>, Option<String>) = sqlx::query_as(
            r#"SELECT
                   (SELECT COALESCE(sum(quantity), 0)::bigint
                      FROM licence_grants
                     WHERE vendor_id = $1 AND (expires_at IS NULL OR expires_at > now())),
                   (SELECT count(*)::bigint
                      FROM licence_activations WHERE vendor_id = $1),
                   (SELECT COALESCE(sum(quantity::bigint * unit_price_cents), 0)::bigint
                      FROM licence_grants
                     WHERE vendor_id = $1 AND (expires_at IS NULL OR expires_at > now())),
                   (SELECT count(*)::bigint
                      FROM licence_activations
                     WHERE vendor_id = $1
                       AND period = (date_trunc('month', now()))::date),
                   (SELECT COALESCE(sum(unit_price_cents), 0)::bigint
                      FROM licence_activations WHERE vendor_id = $1),
                   (SELECT unit_price_cents
                      FROM licence_grants
                     WHERE vendor_id = $1 AND (expires_at IS NULL OR expires_at > now())
                     ORDER BY granted_at DESC LIMIT 1),
                   (SELECT currency
                      FROM licence_grants
                     WHERE vendor_id = $1 AND (expires_at IS NULL OR expires_at > now())
                     ORDER BY granted_at DESC LIMIT 1)"#,
        )
        .bind(vendor_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        let (granted, used, granted_value, this_period, used_value, price, currency) = row;
        Ok(LicenceSummary {
            granted,
            used,
            // Not clamped: a grant revoked after devices were live leaves this negative,
            // and an operator should see that rather than a tidy zero.
            remaining: granted - used,
            activated_this_period: this_period,
            granted_value_cents: granted_value,
            used_value_cents: used_value,
            unit_price_cents: price.unwrap_or(0),
            currency: currency.unwrap_or_else(|| "USD".into()),
        })
    }

    async fn activations(
        &self,
        vendor_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ActivationRow>, AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;
        let rows: Vec<(String, String, String, time::OffsetDateTime, i32)> = sqlx::query_as(
            r#"SELECT a.serial, p.product_key, p.name, a.activated_at, a.unit_price_cents
                 FROM licence_activations a
                 JOIN products p ON p.id = a.product_id
                WHERE a.vendor_id = $1
                ORDER BY a.activated_at DESC
                LIMIT $2"#,
        )
        .bind(vendor_id)
        .bind(limit.clamp(1, 500))
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .map(
                |(serial, product_key, product_name, activated_at, unit_price_cents)| {
                    ActivationRow {
                        serial,
                        product_key,
                        product_name,
                        activated_at,
                        unit_price_cents,
                    }
                },
            )
            .collect())
    }

    async fn grant(&self, grant: &NewGrant) -> Result<Uuid, AppError> {
        grant.validate()?;
        let mut tx = self.pg.scoped(&TenantCtx::system("licence-grant")).await?;

        let id = kernel::new_id();
        sqlx::query(
            r#"INSERT INTO licence_grants
                   (id, vendor_id, quantity, unit_price_cents, currency, expires_at, note)
               VALUES ($1, $2, $3, $4, $5,
                       CASE WHEN $6::bigint IS NULL THEN NULL
                            ELSE now() + ($6::text || ' days')::interval END,
                       $7)"#,
        )
        .bind(id)
        .bind(grant.vendor_id)
        .bind(grant.quantity as i32)
        .bind(grant.unit_price_cents)
        .bind(grant.currency())
        .bind(grant.expires_in_days)
        .bind(grant.note.as_deref())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(id)
    }
}
