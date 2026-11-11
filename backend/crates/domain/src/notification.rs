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
}
