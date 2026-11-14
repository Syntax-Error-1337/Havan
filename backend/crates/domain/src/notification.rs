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
