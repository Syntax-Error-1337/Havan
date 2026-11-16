//! Data subject access: export everything we hold about one person, and erase it on request.
//!
//! Two rules shape this slice, and both are about what *not* to do:
//!
//! - An export is the subject's own data. A co-member's email is a third party's data and does not
//!   appear in it; the export says "role: admin" without saying who.
//! - An erasure is not `DELETE FROM users`. That row owns homes, homes own devices, and devices are
//!   a vendor's activated units — a cascade would unbind hardware from people who did not ask to be
//!   forgotten. Erasure anonymises the person, removes their memberships and their private rows,
//!   and deletes only homes nobody else lives in. A shared home is refused rather than guessed at.

use std::future::Future;

use kernel::AppError;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

/// How much history an export includes. An export that walks a year of telemetry is a file nobody
/// can open; seven days is what a person can actually read, and the rest is described in the body.
pub const EXPORT_TELEMETRY_DAYS: i64 = 7;
pub const EXPORT_TELEMETRY_MAX_ROWS: i64 = 5000;
pub const EXPORT_NOTIFICATION_LIMIT: i64 = 500;

#[derive(Debug, Clone, Serialize)]
pub struct ExportBundle {
    pub format_version: i32,
    pub exported_at: time::OffsetDateTime,
    pub user: Value,
    pub homes: Vec<Value>,
    pub notifications: Vec<Value>,
    pub telemetry: Vec<Value>,
    /// Plain-language notes about what is *not* here, so the file answers the question it raises.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ErasureReport {
    pub homes_deleted: i64,
    pub devices_deleted: i64,
    pub memberships_removed: i64,
    pub notifications_deleted: i64,
    pub sessions_revoked: i64,
    pub push_tokens_removed: i64,
}

pub trait PrivacyRepo {
    fn export(&self, user_id: Uuid) -> impl Future<Output = Result<ExportBundle, AppError>> + Send;

    /// Erase the person, keep the amounts. Refuses with a `409` when they still own a home that
    /// somebody else lives in, because the alternative is deciding for a stranger.
    fn erase(&self, user_id: Uuid) -> impl Future<Output = Result<ErasureReport, AppError>> + Send;
}
