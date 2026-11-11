//! Reading history back.
//!
//! Telemetry is written by the ingest lane and, until now, nothing ever read it.
//! A chart needs a *shape*, not every row: one day of a chatty sensor is tens of
//! thousands of points, most of them identical.

use std::future::Future;

use kernel::AppError;
use serde::Serialize;
use uuid::Uuid;

/// One bar of the chart.
#[derive(Debug, Clone, Serialize)]
pub struct Bucket {
    #[serde(with = "time::serde::rfc3339")]
    pub t: time::OffsetDateTime,
    /// The average over the bucket. For a boolean this is the fraction of the
    /// window it spent true, which is exactly what a "when is the light on"
    /// chart should draw.
    pub v: f64,
    /// How many raw samples this bar stands for. A bar drawn from one sample and
    /// a bar drawn from two thousand look identical, and they are not the same
    /// claim.
    pub n: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct History {
    pub device_id: Uuid,
    pub code: String,
    pub from: time::OffsetDateTime,
    pub to: time::OffsetDateTime,
    pub bucket_secs: i64,
    pub buckets: Vec<Bucket>,
}

/// The chart's resolution. Sixty bars is what fits on a phone without
/// decimating twice.
pub const TARGET_BUCKETS: i64 = 60;

/// A window, in seconds, chosen from the range the client asks for.
pub fn range_secs(range: &str) -> Option<i64> {
    Some(match range {
        "1h" => 3600,
        "1d" => 86_400,
        "7d" => 7 * 86_400,
        "30d" => 30 * 86_400,
        _ => return None,
    })
}

/// Bucket width for a window. Never zero: `ceil` keeps the bar count at or
/// under the target for any range, including sub-minute ones.
pub fn bucket_secs(window: i64) -> i64 {
    ((window + TARGET_BUCKETS - 1) / TARGET_BUCKETS).max(1)
}

pub trait HistoryRepo {
    fn history(
        &self,
        device_id: Uuid,
        user_id: Uuid,
        point_id: Uuid,
        window_secs: i64,
        bucket_secs: i64,
    ) -> impl Future<Output = Result<Vec<Bucket>, AppError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_are_a_closed_set() {
        // An open range string is a full table scan with a user-supplied interval.
        assert_eq!(range_secs("1d"), Some(86_400));
        assert_eq!(range_secs("30d"), Some(30 * 86_400));
        assert_eq!(range_secs("10y"), None);
        assert_eq!(range_secs(""), None);
        assert_eq!(range_secs("1d; DROP TABLE telemetry"), None);
    }

    #[test]
    fn a_day_is_bucketed_into_minutes() {
        assert_eq!(bucket_secs(86_400), 1_440);
    }

    #[test]
    fn the_bucket_count_never_exceeds_the_target() {
        for range in ["1h", "1d", "7d", "30d"] {
            let window = range_secs(range).expect("known range");
            let count = (window + bucket_secs(window) - 1) / bucket_secs(window);
            assert!(count <= TARGET_BUCKETS, "{range} produced {count} buckets");
            assert!(count > 0);
        }
    }

    #[test]
    fn a_shorter_window_is_never_silently_zero() {
        assert_eq!(bucket_secs(1), 1);
        assert_eq!(bucket_secs(0), 1);
    }
}
