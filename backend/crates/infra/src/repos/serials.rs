use domain::serials::{SerialRepo, SerialSummary};
use kernel::{AppError, TenantCtx};
use uuid::Uuid;

use crate::pg::Pg;

#[derive(Clone)]
pub struct PgSerials {
    pg: Pg,
}

impl PgSerials {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    async fn tx(
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
                request_id: "serials".into(),
            })
            .await
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

impl SerialRepo for PgSerials {
    async fn add(
        &self,
        vendor_id: Uuid,
        product_id: Uuid,
        serials: &[String],
    ) -> Result<(i64, i64), AppError> {
        let mut tx = self.tx(vendor_id).await?;

        // One statement for the whole batch: a manifest is uploaded in thousands,
        // and a row-by-row insert would take minutes.
        let inserted = sqlx::query(
            r#"INSERT INTO product_serials (vendor_id, product_id, serial)
               SELECT $1, $2, unnest($3::text[])
               ON CONFLICT (product_id, serial) DO NOTHING"#,
        )
        .bind(vendor_id)
        .bind(product_id)
        .bind(serials)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected() as i64;

        tx.commit().await.map_err(db)?;

        Ok((inserted, serials.len() as i64 - inserted))
    }

    async fn summary(&self, vendor_id: Uuid, product_id: Uuid) -> Result<SerialSummary, AppError> {
        let mut tx = self.tx(vendor_id).await?;

        let (total, claimed): (i64, i64) = sqlx::query_as(
            r#"SELECT count(*),
                      count(*) FILTER (WHERE claimed_at IS NOT NULL)
                 FROM product_serials
                WHERE product_id = $1"#,
        )
        .bind(product_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(SerialSummary {
            total,
            claimed,
            open: total - claimed,
        })
    }
}
