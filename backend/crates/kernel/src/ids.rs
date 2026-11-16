//! Identifiers and clocks.
//!
//! UUIDv7 everywhere: sortable by creation time, which keeps b-tree inserts
//! append-only and makes `ORDER BY id` mean `ORDER BY created_at` for free.

use time::OffsetDateTime;
use uuid::Uuid;

/// A new time-ordered id. Monotonic within a process, sortable across them.
pub fn new_id() -> Uuid {
    Uuid::now_v7()
}

pub fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// The current time as RFC 3339, for JSON.
///
/// `json!` builds through `serde_json::value::Serializer`, which reports itself as *not*
/// human-readable — so `OffsetDateTime` takes its compact tuple form and a caller receives
/// `[2026,320,18,44,55,...]` where a date should be. Typed response structs are unaffected (SQLx's
/// own mapping emits the string), which is why this only ever showed up in hand-built documents:
/// the webhook body, the support bundle and the DSAR export.
pub fn now_rfc3339() -> String {
    now()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| now().unix_timestamp().to_string())
}

/// Epoch milliseconds, the unit every API boundary uses.
pub fn now_ms() -> i64 {
    now().unix_timestamp_nanos() as i64 / 1_000_000
}

/// Epoch seconds, the unit the device wire protocol uses.
pub fn now_s() -> i64 {
    now().unix_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_sort_by_creation() {
        let a = new_id();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = new_id();
        assert!(a < b, "{a} should sort before {b}");
        assert_eq!(a.get_version_num(), 7);
    }

    #[test]
    fn clocks_agree() {
        let s = now_s();
        let ms = now_ms();
        assert!((ms / 1000 - s).abs() <= 1);
    }
}
