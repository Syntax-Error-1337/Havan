use domain::webhook::{MAX_ATTEMPTS, PendingDelivery, WebhookRepo, WebhookSubscription};
use kernel::{AppError, ErrorCode, TenantCtx};
use uuid::Uuid;

use crate::pg::Pg;

#[derive(Clone)]
pub struct PgWebhooks {
    pg: Pg,
}

impl PgWebhooks {
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
                request_id: "webhooks".into(),
            })
            .await
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

impl WebhookRepo for PgWebhooks {
    async fn create(
        &self,
        vendor_id: Uuid,
        url: &str,
        events: Vec<String>,
    ) -> Result<(Uuid, String), AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;

        // A webhook to a non-https endpoint sends a signing secret in the clear over the network,
        // so it is refused unless it is loopback (a vendor testing locally) or TLS.
        let acceptable = url.starts_with("https://")
            || url.starts_with("http://127.0.0.1")
            || url.starts_with("http://localhost");
        if !acceptable {
            return Err(AppError::validation(
                "webhook URLs must be https, or http on localhost for testing",
            ));
        }

        let secret = format!(
            "whsec_{}{}",
            Uuid::now_v7().simple(),
            Uuid::now_v7().simple()
        );
        let id = kernel::new_id();

        sqlx::query(
            "INSERT INTO vendor_webhooks (id, vendor_id, url, secret, events) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(vendor_id)
        .bind(url)
        .bind(&secret)
        .bind(&events)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if e.as_database_error()
                .is_some_and(|d| d.code().as_deref() == Some("23505"))
            {
                AppError::code(ErrorCode::Conflict, "that URL is already subscribed")
            } else {
                db(e)
            }
        })?;

        tx.commit().await.map_err(db)?;
        Ok((id, secret))
    }

    async fn list(&self, vendor_id: Uuid) -> Result<Vec<WebhookSubscription>, AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;
        let rows: Vec<(Uuid, String, Vec<String>, bool, time::OffsetDateTime)> = sqlx::query_as(
            "SELECT id, url, events, active, created_at FROM vendor_webhooks
              WHERE vendor_id = $1 ORDER BY created_at DESC",
        )
        .bind(vendor_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .map(
                |(id, url, events, active, created_at)| WebhookSubscription {
                    id,
                    url,
                    events,
                    active,
                    created_at,
                },
            )
            .collect())
    }

    async fn delete(&self, vendor_id: Uuid, id: Uuid) -> Result<(), AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;
        let n = sqlx::query("DELETE FROM vendor_webhooks WHERE vendor_id = $1 AND id = $2")
            .bind(vendor_id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        tx.commit().await.map_err(db)?;

        if n == 0 {
            return Err(AppError::not_found("webhook"));
        }
        Ok(())
    }

    async fn due(&self, limit: i64) -> Result<Vec<PendingDelivery>, AppError> {
        let mut tx = self
            .pg
            .scoped(&TenantCtx::system("webhook-deliver"))
            .await?;

        // Only the first active subscription per event is used per row: one outbox row is one
        // delivery attempt, and a vendor with two endpoints for the same event gets two rows at
        // enqueue time if they asked for two.
        let rows: Vec<(Uuid, Uuid, String, String, String, serde_json::Value, i32)> =
            sqlx::query_as(
                r#"SELECT o.id, o.vendor_id, w.url, w.secret, o.event, o.payload, o.attempts
                     FROM webhook_outbox o
                     JOIN vendor_webhooks w
                       ON w.vendor_id = o.vendor_id AND w.active AND o.event = ANY (w.events)
                    WHERE o.delivered_at IS NULL
                      AND o.attempts < $2
                      AND o.next_attempt_at <= now()
                    ORDER BY o.created_at
                    LIMIT $1
                    FOR UPDATE OF o SKIP LOCKED"#,
            )
            .bind(limit)
            .bind(MAX_ATTEMPTS)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .map(
                |(id, vendor_id, url, secret, event, payload, attempts)| PendingDelivery {
                    id,
                    vendor_id,
                    url,
                    secret,
                    event,
                    payload,
                    attempts,
                },
            )
            .collect())
    }

    async fn mark_delivered(&self, id: Uuid) -> Result<(), AppError> {
        let mut tx = self
            .pg
            .scoped(&TenantCtx::system("webhook-deliver"))
            .await?;
        sqlx::query(
            "UPDATE webhook_outbox SET delivered_at = now(), last_error = NULL WHERE id = $1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn mark_failed(&self, id: Uuid, error: &str) -> Result<(), AppError> {
        let mut tx = self
            .pg
            .scoped(&TenantCtx::system("webhook-deliver"))
            .await?;

        // 1, 2, 4, … capped at an hour: a vendor whose endpoint is down for the afternoon gets
        // retries, and one whose endpoint is gone does not get hammered for a week.
        sqlx::query(
            r#"UPDATE webhook_outbox
                  SET attempts = attempts + 1,
                      last_error = $2,
                      next_attempt_at = now() + least(
                          make_interval(secs => power(2, attempts)::int), interval '1 hour')
                WHERE id = $1"#,
        )
        .bind(id)
        .bind(error)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)
    }
}
