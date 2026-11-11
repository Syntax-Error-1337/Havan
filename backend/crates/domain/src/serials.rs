//! The manufacturing manifest: which serials a vendor says it built.
//!
//! Without it, claiming has to trust whoever presents a ticket — which makes a
//! device row something a caller can invent. With it, "this serial exists" is a
//! fact the vendor asserted at manufacturing time.

use std::future::Future;

use kernel::AppError;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SerialSummary {
    pub total: i64,
    pub claimed: i64,
    pub open: i64,
}

pub trait SerialRepo {
    /// Add a batch. Returns `(added, already_listed)` — uploading the same
    /// manifest twice is a normal mistake and must not be an error.
    fn add(
        &self,
        vendor_id: Uuid,
        product_id: Uuid,
        serials: &[String],
    ) -> impl Future<Output = Result<(i64, i64), AppError>> + Send;

    fn summary(
        &self,
        vendor_id: Uuid,
        product_id: Uuid,
    ) -> impl Future<Output = Result<SerialSummary, AppError>> + Send;
}

/// Trim, drop blanks and de-duplicate.
///
/// A manifest pasted out of a spreadsheet has trailing spaces and the occasional
/// repeated line; refusing the whole upload over them would be hostile, and
/// storing them verbatim would make the serial never match a device.
pub fn normalise(serials: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for serial in serials {
        let trimmed = serial.trim();
        if trimmed.is_empty() || trimmed.len() > 128 {
            continue;
        }
        if !out.iter().any(|s| s == trimmed) {
            out.push(trimmed.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_drops_blanks_and_deduplicates() {
        let out = normalise(vec![
            " SN-1 ".into(),
            "SN-2".into(),
            "SN-1".into(),
            "".into(),
            "   ".into(),
        ]);
        assert_eq!(out, vec!["SN-1".to_string(), "SN-2".to_string()]);
    }

    #[test]
    fn refuses_absurd_serials_but_keeps_the_end_of_bad_data() {
        let long = "x".repeat(129);
        let out = normalise(vec![long, "SN-3".into()]);
        assert_eq!(out, vec!["SN-3".to_string()]);
    }
}
