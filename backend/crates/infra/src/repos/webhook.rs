use domain::webhook::{
    MAX_ATTEMPTS, PendingDelivery, WebhookDelivery, WebhookRepo, WebhookSubscription,
};
use kernel::{AppError, ErrorCode, TenantCtx};
use uuid::Uuid;

use crate::pg::Pg;

#[derive(Clone)]
pub struct PgWebhooks {
    pg: Pg,
}

/// One row of the list query: the subscription, then the newest attempt's `(at, delivered, attempts, error)`.
///
/// Named rather than written inline because a nine-tuple in a `let` is a puzzle, and clippy is right to say so.
type SubscriptionRow = (
    Uuid,
    String,
    Vec<String>,
    bool,
    time::OffsetDateTime,
    Option<time::OffsetDateTime>,
    Option<bool>,
    Option<i32>,
    Option<String>,
);

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
        // The newest outbox row per endpoint, in the same round trip. `LATERAL` keeps it one row per subscription: a
        // plain join would multiply the list by its delivery history, which is the bug this shape exists to avoid.
        let rows: Vec<SubscriptionRow> = sqlx::query_as(
            r#"SELECT w.id, w.url, w.events, w.active, w.created_at,
                      o.created_at, o.delivered_at IS NOT NULL, o.attempts, o.last_error
                 FROM vendor_webhooks w
                 LEFT JOIN LATERAL (
                      SELECT created_at, delivered_at, attempts, last_error
                        FROM webhook_outbox
                       WHERE subscription_id = w.id
                       ORDER BY created_at DESC
                       LIMIT 1
                 ) o ON true
                WHERE w.vendor_id = $1
                ORDER BY w.created_at DESC"#,
        )
        .bind(vendor_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .map(
                |(id, url, events, active, created_at, at, delivered, attempts, error)| {
                    WebhookSubscription {
                        id,
                        url,
                        events,
                        active,
                        created_at,
                        last_delivery: at.map(|at| WebhookDelivery {
                            at,
                            delivered: delivered.unwrap_or(false),
                            attempts: attempts.unwrap_or(0),
                            error,
                        }),
                    }
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

        // One row, one subscription. The row names the subscription it was written for, because
        // re-joining by `event = ANY (w.events)` matched *every* active endpoint: with five
        // subscriptions that produced five deliveries per row sharing one outbox id, and
        // `mark_delivered` fired on the first success — so four endpoints never heard the event and
        // which one did depended on join order. `o.subscription_id` is that fix.
        // The claim is a **lease**, not a lock. `FOR UPDATE SKIP LOCKED` released by the commit
        // below would hold nothing while the HTTP request is in flight, so two worker replicas
        // could pick the same row and deliver the event twice — the same defect the command
        // redelivery path had, found the same way. Pushing `next_attempt_at` forward claims the row
        // for the length of one delivery; a worker that dies mid-send leaves a lease that simply
        // expires.
        let rows: Vec<(Uuid, Uuid, String, String, String, serde_json::Value, i32)> =
            sqlx::query_as(
                r#"WITH claimed AS (
                       SELECT id
                         FROM webhook_outbox
                        WHERE delivered_at IS NULL
                          AND attempts < $2
                          AND next_attempt_at <= now()
                        ORDER BY created_at
                        LIMIT $1
                        FOR UPDATE SKIP LOCKED
                   )
                   UPDATE webhook_outbox o
                      -- Five minutes, not sixty seconds: one pass leases BATCH rows and sends them
                      -- one at a time, so the worst case is 20 × the 10 s request timeout. A lease
                      -- shorter than the batch would hand its own tail to a second worker.
                      SET next_attempt_at = now() + interval '5 minutes'
                     FROM claimed c, vendor_webhooks w
                    WHERE o.id = c.id
                      AND w.id = o.subscription_id
                      AND w.active
                   RETURNING o.id, o.vendor_id, w.url, w.secret, o.event, o.payload, o.attempts"#,
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
