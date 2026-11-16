//! Webhook delivery: the worker half of the transactional outbox.
//!
//! Rows land in `webhook_outbox` inside the transaction that caused them, so nothing here is on a
//! request path. This loop claims a batch, signs it, posts it, and records the outcome.
//!
//! Failure is the normal case for a webhook — an endpoint is down, a URL moved, a vendor's staging
//! server is asleep — so the loop's health is measured by what it does with a failure: the row keeps
//! its attempt count, its backoff doubles, and after `MAX_ATTEMPTS` it stops being retried with the
//! last error kept for the vendor to read.

use std::time::Duration;

use domain::webhook::{WebhookRepo, sign};
use infra::{Pg, PgWebhooks};
use tracing::warn;

const BATCH: i64 = 20;
const TICK: Duration = Duration::from_secs(5);
const TIMEOUT: Duration = Duration::from_secs(10);

pub async fn run(pg: Pg) {
    let repo = PgWebhooks::new(pg.clone());
    let client = match reqwest::Client::builder().timeout(TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "webhook client could not be built; delivery disabled");
            return;
        }
    };

    let mut ticker = tokio::time::interval(TICK);
    loop {
        ticker.tick().await;

        let batch = match repo.due(BATCH).await {
            Ok(b) => b,
            Err(e) => {
                warn!(error = %e, "webhook outbox could not be read");
                continue;
            }
        };

        for delivery in batch {
            let timestamp = kernel::now().unix_timestamp();
            let body = serde_json::json!({
                "id": delivery.id,
                "event": delivery.event,
                "created_at": kernel::now(),
                "data": delivery.payload,
            });
            let bytes = serde_json::to_vec(&body).unwrap_or_default();
            let signature = sign(&delivery.secret, timestamp, &bytes);

            let response = client
                .post(&delivery.url)
                .header("content-type", "application/json")
                .header("x-haven-event", &delivery.event)
                .header("x-haven-timestamp", timestamp.to_string())
                .header("x-haven-signature", &signature)
                // The id makes the delivery idempotent on the receiver's side: a retry after a
                // timeout that actually succeeded is the same event, not a second one.
                .header("x-haven-delivery", delivery.id.to_string())
                .body(bytes)
                .send()
                .await;

            let outcome = match response {
                Ok(r) if r.status().is_success() => Ok(()),
                Ok(r) => Err(format!("endpoint answered {}", r.status())),
                Err(e) => Err(format!("request failed: {e}")),
            };

            match outcome {
                Ok(()) => {
                    if let Err(e) = repo.mark_delivered(delivery.id).await {
                        warn!(error = %e, "a delivered webhook could not be marked");
                    }
                }
                Err(error) => {
                    warn!(
                        event = %delivery.event,
                        vendor = %delivery.vendor_id,
                        attempt = delivery.attempts + 1,
                        %error,
                        "webhook delivery failed"
                    );
                    if let Err(e) = repo.mark_failed(delivery.id, &error).await {
                        warn!(error = %e, "a failed webhook could not be recorded");
                    }
                }
            }
        }
    }
}
