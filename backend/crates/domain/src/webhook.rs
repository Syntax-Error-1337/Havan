//! Outbound events: what a vendor's own systems are told, and how they know it came from us.
//!
//! Two rules decide the design:
//!
//! - **A webhook never blocks the thing that caused it.** `licence.activated` is written to an
//!   outbox in the same transaction as the activation, and delivered later by a worker. A vendor
//!   with a slow endpoint cannot slow down a customer's pairing, and a vendor with no endpoint
//!   costs one row.
//! - **A signature covers the timestamp as well as the body.** Signing only the body lets anyone
//!   replay yesterday's valid delivery forever; signing `timestamp.body` with the timestamp in a
//!   header means a receiver can reject anything older than a few minutes.
//!
//! The events are named as facts in the past tense, because that is what they are: something
//! happened, and this is the record of it.

use std::future::Future;

use hmac::{Hmac, Mac};
use kernel::AppError;
use serde::Serialize;
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

pub const EVENT_LICENCE_ACTIVATED: &str = "licence.activated";
pub const EVENT_DEVICE_CLAIMED: &str = "device.claimed";
pub const EVENT_FIRMWARE_FAILED: &str = "firmware.failed";

/// Every event a vendor may subscribe to. An unknown event name is refused at subscription time
/// rather than silently never delivered.
pub const EVENTS: [&str; 3] = [
    EVENT_LICENCE_ACTIVATED,
    EVENT_DEVICE_CLAIMED,
    EVENT_FIRMWARE_FAILED,
];

/// Deliveries attempted before a subscription is left to fail loudly in the vendor's own logs.
pub const MAX_ATTEMPTS: i32 = 12;

pub fn is_known_event(event: &str) -> bool {
    EVENTS.contains(&event)
}

/// `sha256=<hex>` over `timestamp.body`, with the timestamp sent as `x-haven-timestamp`.
///
/// The receiver recomputes this with the secret it was given once, compares in constant time, and
/// rejects a timestamp that is not within its own window.
pub fn sign(secret: &str, timestamp: i64, body: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    format!("sha256={:x}", mac.finalize().into_bytes())
}

#[derive(Debug, Clone, Serialize)]
pub struct WebhookSubscription {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    pub active: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    /// The most recent attempt, if there has ever been one.
    ///
    /// A vendor cannot otherwise tell whether their endpoint is working: an endpoint that has been refusing for a week
    /// looks exactly like one nobody has needed yet. Absent means nothing has been sent — which is also worth saying.
    pub last_delivery: Option<WebhookDelivery>,
}

/// The newest attempt for one endpoint: when, whether it landed, and what went wrong if it did not.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookDelivery {
    #[serde(with = "time::serde::rfc3339")]
    pub at: time::OffsetDateTime,
    pub delivered: bool,
    pub attempts: i32,
    pub error: Option<String>,
}

impl WebhookDelivery {
    /// One sentence a vendor can act on. The status line reuses it, so the API and the portal cannot disagree.
    pub fn describe(&self) -> String {
        if self.delivered {
            format!(
                "delivered after {} attempt{}",
                self.attempts,
                plural(self.attempts)
            )
        } else if let Some(error) = &self.error {
            format!(
                "failed {} time{}: {error}",
                self.attempts,
                plural(self.attempts)
            )
        } else {
            format!(
                "waiting to retry, {} attempt{} so far",
                self.attempts,
                plural(self.attempts)
            )
        }
    }
}

fn plural(n: i32) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[derive(Debug, Clone)]
pub struct PendingDelivery {
    pub id: Uuid,
    pub vendor_id: Uuid,
    pub url: String,
    pub secret: String,
    pub event: String,
    pub payload: serde_json::Value,
    pub attempts: i32,
}

pub trait WebhookRepo {
    /// Returns the subscription id and the signing secret, which is shown once.
    fn create(
        &self,
        vendor_id: Uuid,
        url: &str,
        events: Vec<String>,
    ) -> impl Future<Output = Result<(Uuid, String), AppError>> + Send;

    fn list(
        &self,
        vendor_id: Uuid,
    ) -> impl Future<Output = Result<Vec<WebhookSubscription>, AppError>> + Send;

    fn delete(
        &self,
        vendor_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// The next batch of undelivered events whose backoff has elapsed. System-scoped: it crosses
    /// vendors because the worker serves all of them.
    fn due(
        &self,
        limit: i64,
    ) -> impl Future<Output = Result<Vec<PendingDelivery>, AppError>> + Send;

    fn mark_delivered(&self, id: Uuid) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Record a failed attempt and schedule the next one with exponential backoff.
    fn mark_failed(
        &self,
        id: Uuid,
        error: &str,
    ) -> impl Future<Output = Result<(), AppError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_signature_matches_a_standard_hmac() {
        // Computed independently with Python's hmac/hashlib. If this changes, every vendor's
        // verifier breaks at once — which is exactly what a test should catch before they do.
        let sig = sign("secret", 1_700_000_000, b"{\"event\":\"x\"}");
        assert_eq!(
            sig,
            "sha256=ca458d767c4041a70394d25abfdb6d6bc3777d2044b7dcf28fcc31911996fde4"
        );

        // Deterministic.
        assert_eq!(sig, sign("secret", 1_700_000_000, b"{\"event\":\"x\"}"));
    }

    #[test]
    fn the_timestamp_is_part_of_what_is_signed() {
        let a = sign("secret", 1_700_000_000, b"{}");
        let b = sign("secret", 1_700_000_001, b"{}");
        assert_ne!(a, b, "a changed timestamp must change the signature");
    }

    #[test]
    fn a_different_secret_produces_a_different_signature() {
        assert_ne!(sign("one", 1, b"{}"), sign("two", 1, b"{}"));
    }

    #[test]
    fn only_known_events_can_be_subscribed_to() {
        assert!(is_known_event(EVENT_LICENCE_ACTIVATED));
        assert!(is_known_event(EVENT_DEVICE_CLAIMED));
        assert!(!is_known_event("licence.maybe"));
    }
}
