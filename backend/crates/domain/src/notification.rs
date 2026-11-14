//! Alerts.
//!
//! The automation engine has been writing rows into `notifications` since it was
//! built — over a thousand of them in this database — and nothing has ever shown
//! one to a person. These are read-and-acknowledge, not delivery: push belongs to
//! a device token, and that is a later problem.

use std::future::Future;

use kernel::AppError;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    pub id: Uuid,
    /// `automation`, `ota`, `security`, …
    pub kind: String,
    pub title: String,
    pub body: String,
    pub data: serde_json::Value,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub read_at: Option<time::OffsetDateTime>,
}

pub trait NotificationRepo {
    /// Newest first. `unread_only` is a second query rather than a filter on the
    /// client: a badge needs the count, not the rows.
    fn list(
        &self,
        user_id: Uuid,
        unread_only: bool,
        limit: i64,
    ) -> impl Future<Output = Result<Vec<Notification>, AppError>> + Send;

    fn unread_count(&self, user_id: Uuid) -> impl Future<Output = Result<i64, AppError>> + Send;

    /// Marking an already-read notification is not an error: two phones opening
    /// the same alert is normal, and the second one should not see a failure.
    fn mark_read(
        &self,
        user_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = Result<bool, AppError>> + Send;

    fn mark_all_read(&self, user_id: Uuid) -> impl Future<Output = Result<u64, AppError>> + Send;

    /// Tell every member of a home that a device's certificate is running out.
    ///
    /// Deduped per (device, warning bucket): the sweep runs on a timer, and a person who has
    /// already been told "14 days left" must not be told again every six hours for a day. The
    /// bucket is part of the payload so the same device can be warned again at 7 and at 0.
    fn notify_certificate_expiry(
        &self,
        device: &CertificateWarning,
    ) -> impl Future<Output = Result<u64, AppError>> + Send;
}

/// What the sweep has to say about one device.
#[derive(Debug, Clone)]
pub struct CertificateWarning {
    pub device_id: Uuid,
    pub home_id: Option<Uuid>,
    pub name: String,
    pub serial: String,
    pub days_left: i64,
    pub bucket: i64,
}

impl CertificateWarning {
    pub fn title(&self) -> String {
        if self.days_left <= 0 {
            format!("{} can no longer connect", self.name)
        } else {
            format!("{} stops connecting in {} days", self.name, self.days_left)
        }
    }

    /// The body says what to do, not what happened: a warning that does not name an action
    /// is a warning people learn to dismiss.
    pub fn body(&self) -> String {
        if self.days_left <= 0 {
            format!(
                "The certificate for {} ({}) has expired. Re-enrol the device to restore its connection.",
                self.name, self.serial
            )
        } else {
            format!(
                "The certificate for {} ({}) expires in {} days. Re-enrol the device before then, or it will drop off the platform.",
                self.name, self.serial, self.days_left
            )
        }
    }
}

/// A notification that still has to reach a phone.
#[derive(Debug, Clone)]
pub struct Delivery {
    pub id: Uuid,
    pub user_id: Uuid,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub data: serde_json::Value,
}

/// A device token, and who it belongs to.
#[derive(Debug, Clone)]
pub struct PushToken {
    pub id: Uuid,
    pub user_id: Uuid,
    /// `ios` or `android` — the provider is chosen from this, not guessed from the token.
    pub platform: String,
    pub token: String,
    pub app_version: Option<String>,
}

/// How many times one notification may be attempted before it is abandoned.
///
/// Three, over the sweep interval: enough to ride out a token refresh or a provider hiccup,
/// few enough that a permanently broken token does not sit in the queue for a day.
pub const MAX_ATTEMPTS: i32 = 3;

pub trait PushRepo {
    /// Undelivered notifications, oldest first, that are still worth trying.
    fn undelivered(
        &self,
        limit: i64,
    ) -> impl Future<Output = Result<Vec<Delivery>, AppError>> + Send;

    fn tokens_for(
        &self,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Vec<PushToken>, AppError>> + Send;

    /// Nothing left to do: delivered, or nobody to deliver to.
    fn mark_delivered(&self, id: Uuid) -> impl Future<Output = Result<(), AppError>> + Send;

    /// A failure worth retrying. After [`MAX_ATTEMPTS`] the row is settled as undeliverable
    /// rather than retried forever.
    fn mark_attempt(
        &self,
        id: Uuid,
        error: &str,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// The provider said this token is dead. Deleting it is the only correct response.
    fn prune_token(&self, id: Uuid) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Register or refresh a device token. Idempotent: an app that registers on every launch
    /// must not accumulate rows.
    fn register_token(
        &self,
        user_id: Uuid,
        platform: &str,
        token: &str,
        app_version: Option<&str>,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn remove_token(
        &self,
        user_id: Uuid,
        token: &str,
    ) -> impl Future<Output = Result<bool, AppError>> + Send;
}
