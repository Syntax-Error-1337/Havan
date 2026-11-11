use domain::history::{Bucket, HistoryRepo};
use kernel::AppError;

use crate::pg::Pg;

#[derive(Clone)]
pub struct PgHistory {
    pg: Pg,
}

impl PgHistory {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

impl HistoryRepo for PgHistory {
    async fn history(
        &self,
        device_id: uuid::Uuid,
        user_id: uuid::Uuid,
        point_id: uuid::Uuid,
        window_secs: i64,
        bucket_secs: i64,
    ) -> Result<Vec<Bucket>, AppError> {
        // Scoped by *user*, not vendor: this is a customer reading their own
        // device, and the RLS policy on telemetry checks home membership.
        let mut tx = self
            .pg
            .scoped(&kernel::TenantCtx {
                vendor_id: None,
                actor: kernel::Actor::User {
                    user_id,
                    vendor_id: None,
                },
                request_id: "history".into(),
            })
            .await?;

        // `bool` and `num` live in different columns; a chart of a switch wants
        // the fraction of the window it was on, which is the same average.
        let rows = sqlx::query_as::<_, (time::OffsetDateTime, f64, i64)>(
            r#"SELECT to_timestamp(floor(extract(epoch FROM ts) / $4) * $4) AS bucket,
                      avg(COALESCE(num, bool::int::float8)) AS v,
                      count(*) AS n
                 FROM telemetry
                WHERE device_id = $1
                  AND point_id = $2
                  AND ts >= now() - make_interval(secs => $3)
                  AND ts <= now()
                  AND (num IS NOT NULL OR bool IS NOT NULL)
                GROUP BY bucket
                ORDER BY bucket"#,
        )
        .bind(device_id)
        .bind(point_id)
        .bind(window_secs as f64)
        .bind(bucket_secs as f64)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .map(|(t, v, n)| Bucket { t, v, n })
            .collect())
    }
}
