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
    /// RFC 3339, like every other timestamp the API returns. Without this attribute `time`
    /// serialises as a tuple — `[2026,320,18,45,31,...]` — and the file a person is legally owed
    /// arrives with a date no reader can parse.
    #[serde(with = "time::serde::rfc3339")]
    pub exported_at: time::OffsetDateTime,
    pub user: Value,
    pub homes: Vec<Value>,
    pub notifications: Vec<Value>,
    pub telemetry: Vec<Value>,
    /// Plain-language notes about what is *not* here, so the file answers the question it raises.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ErasureReport {
    pub homes_deleted: i64,
    pub devices_deleted: i64,
    pub memberships_removed: i64,
    pub notifications_deleted: i64,
    pub sessions_revoked: i64,
    pub push_tokens_removed: i64,
    /// The vendor's retention window that applies to this person, from the licence they were
    /// covered by. Reported rather than applied silently: the person is entitled to know when the
    /// receipt of their request disappears.
    pub retention_days: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub purge_after: time::OffsetDateTime,
}

/// What a purge run did. Ids, not counts: a deletion nobody can name is a deletion nobody can
/// audit.
#[derive(Debug, Clone, Serialize)]
pub struct PurgeReport {
    pub purged: Vec<Uuid>,
}

pub trait PrivacyRepo {
    fn export(&self, user_id: Uuid) -> impl Future<Output = Result<ExportBundle, AppError>> + Send;

    /// Erase the person, keep the amounts. Refuses with a `409` when they still own a home that
    /// somebody else lives in, because the alternative is deciding for a stranger.
    fn erase(&self, user_id: Uuid) -> impl Future<Output = Result<ErasureReport, AppError>> + Send;

    /// Delete every tombstone whose retention window has passed, and leave a receipt for each.
    ///
    /// `limit` bounds one run so a backlog is worked through in batches instead of holding a
    /// transaction open for a thousand rows while the rest of the platform waits.
    /// How many tombstones are past their retention window right now.
    ///
    /// The operator console needs this before the button: "purge" with no number in front of it is a
    /// command nobody can sanity-check, and a number that is not zero is the finding.
    fn purging_soon(&self) -> impl Future<Output = Result<i64, AppError>> + Send;

    fn purge_due(&self, limit: i64) -> impl Future<Output = Result<PurgeReport, AppError>> + Send;
}
