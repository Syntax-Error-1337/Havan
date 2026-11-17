//! Keeping `telemetry` a partitioned table, forever.
//!
//! Migration 0003 created twenty-four monthly partitions and a DEFAULT partition for anything with a
//! clock that disagrees with the calendar. Nothing has created one since, and nothing has ever dropped
//! one:
//!
//! * after the twenty-four months run out, every reading lands in `telemetry_overflow` — it still works,
//!   and every query loses partition pruning, which is the whole reason the table is partitioned;
//! * every reading ever written is still there. At a million devices reporting every minute, that is
//!   1.4 billion rows a day, and the first symptom is a disk that fills at 3 a.m.
//!
//! Both are the same job: keep the window rolling. Creating a month ahead is a metadata operation;
//! dropping a month is `DROP TABLE` on one partition, not a mass `DELETE` — which is exactly why the
//! table is partitioned rather than just indexed.
//!
//! ## The rows already stranded in the overflow
//!
//! 0003 started partitions at the month it ran in, so anything written for the month *before* had
//! nowhere to go. The first run of this job against a real database therefore does not find an empty
//! overflow — it finds the previous month's readings sitting in it, and Postgres refuses to create a
//! partition whose bounds would claim rows the DEFAULT partition already holds:
//!
//! ```text
//! updated partition constraint for default partition "telemetry_overflow" would be violated by some row
//! ```
//!
//! Lifting those rows out first (a temp spill, then the new partition, then the spill back in) is what
//! makes the job idempotent on a platform that has already been running: it repairs the month that was
//! missed as a side effect of creating the month that is due.

use kernel::AppError;
use sqlx::PgPool;
use time::{Date, Duration, Month, OffsetDateTime};

use crate::pg::Pg;

/// How many months ahead to keep materialised. Three is enough that a worker which was down for a
/// weekend still cannot be the reason an insert lands in the overflow.
const MONTHS_AHEAD: i64 = 3;

/// Raw readings older than this are dropped, partition by partition.
///
/// Two years is a platform-wide default rather than a per-vendor term: a vendor's `retention_days` is
/// about *people* (how long a deleted account's receipt survives), and applying it to telemetry would
/// let one vendor's contract silently destroy the history of a device belonging to another. A deployment
/// that wants a shorter window passes it in.
pub const DEFAULT_RETENTION_DAYS: i64 = 730;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MaintenanceReport {
    pub created: Vec<String>,
    pub dropped: Vec<String>,
    /// Readings rescued from the DEFAULT partition into the month partition they belonged to.
    pub rescued: i64,
    /// Rows still in the DEFAULT partition after the run. A non-zero number here means a device with a
    /// broken clock, which an operator should see rather than infer from a query plan.
    pub overflow_rows: i64,
    /// Hourly rollup rows deleted. `telemetry_rollup` is not partitioned — one row per point per hour is
    /// three orders of magnitude smaller than the readings it summarises — so it is deleted in batches
    /// rather than dropped by partition.
    pub rollups_deleted: i64,
}

/// How many rollup rows one statement may delete. A million-row `DELETE` holds a transaction for as long
/// as it takes to write a million tombstones; forty thousand is a few milliseconds and still eats a day of
/// expired history per tick, so a backlog drains without ever being visible to a reader.
const ROLLUP_BATCH: i64 = 40_000;

/// The most batches one tick will run, so a first run against a database that has never been pruned does
/// not hold the worker for an hour. The next tick picks up where this one stopped.
const ROLLUP_BATCHES_PER_RUN: usize = 25;

/// Create the partitions the calendar is about to need, and drop the ones past the window.
///
/// The whole run is one transaction on one connection: the spill table is temporary, so it lives exactly
/// as long as this session, and a pool that handed each statement a different connection would lose it
/// between them.
///
/// `sqlx::query`, not `sqlx::raw_sql`: every statement here is a single one, and the future built from
/// `raw_sql` is not `Send` — which the compiler reports, forty lines away and two crates up, as
/// "implementation of Executor is not general enough" at the `tokio::spawn` in `worker`.
pub async fn maintain(pg: &Pg, retention_days: i64) -> Result<MaintenanceReport, AppError> {
    let mut tx = pg.pool().begin().await.map_err(db)?;

    sqlx::query("CREATE TEMP TABLE IF NOT EXISTS telemetry_spill (LIKE telemetry) ON COMMIT DROP")
        .execute(&mut *tx)
        .await
        .map_err(db)?;

    let mut report = MaintenanceReport {
        created: Vec::new(),
        dropped: Vec::new(),
        rescued: 0,
        overflow_rows: 0,
        rollups_deleted: 0,
    };

    // One month back as well. A worker that first runs at 00:05 on the 1st must not leave the month it is
    // standing in missing, and — as 0003 proved — the month a platform started in is exactly the month
    // somebody will have data for and no partition to put it in.
    for offset in -1..=MONTHS_AHEAD {
        let start = shift_months(current_month(), offset);
        let end = shift_months(start, 1);
        let name = partition_name(start);

        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(&name)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
        if exists {
            continue;
        }

        // Out of the way first: Postgres will not let a new partition claim rows the DEFAULT partition
        // already holds, so the ones that belong to this month are lifted into the spill table.
        let rescued = sqlx::query(
            r#"WITH moved AS (
                   DELETE FROM telemetry_overflow
                    WHERE ts >= $1 AND ts < $2
                   RETURNING ts, device_id, point_id, vendor_id, num, bool, txt
               )
               INSERT INTO telemetry_spill SELECT * FROM moved"#,
        )
        .bind(start.midnight().assume_utc())
        .bind(end.midnight().assume_utc())
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        report.rescued += i64::try_from(rescued).unwrap_or(i64::MAX);

        // Name and bounds are computed here as dates, never assembled from a caller's string, and the
        // name is re-checked against `month_of` before it is ever formatted into SQL.
        debug_assert!(month_of(&name).is_some(), "partition name is a month");
        let statement = format!(
            "CREATE TABLE IF NOT EXISTS \"{name}\" PARTITION OF telemetry \
             FOR VALUES FROM ('{start}') TO ('{end}')",
            start = start.midnight().assume_utc(),
            end = end.midnight().assume_utc(),
        );
        sqlx::query(&statement)
            .execute(&mut *tx)
            .await
            .map_err(db)?;

        // Back in through the parent, so every row finds the month partition that now exists.
        sqlx::query("INSERT INTO telemetry SELECT * FROM telemetry_spill")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        sqlx::query("TRUNCATE telemetry_spill")
            .execute(&mut *tx)
            .await
            .map_err(db)?;

        report.created.push(name);
    }

    // Names are matched, not bounds parsed: a partition this job did not create is not this job's to
    // drop. The DEFAULT partition never matches, because it has no month in its name.
    let candidates: Vec<String> = sqlx::query_scalar(
        r#"SELECT c.relname
             FROM pg_class c
             JOIN pg_inherits i ON i.inhrelid = c.oid
            WHERE i.inhparent = 'telemetry'::regclass
              AND c.relname ~ '^telemetry_[0-9]{4}_[0-9]{2}$'"#,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(db)?;

    let cutoff = OffsetDateTime::now_utc() - Duration::days(retention_days);
    for name in candidates {
        let Some(month_start) = month_of(&name) else {
            continue;
        };
        // A partition goes once the *end* of its month is past the window: half a month of data that is
        // still inside the retention promise is not this job's to delete.
        let month_end = shift_months(month_start, 1).midnight().assume_utc();
        if month_end >= cutoff {
            continue;
        }

        let statement = format!("DROP TABLE IF EXISTS \"{name}\"");
        sqlx::query(&statement)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        report.dropped.push(name);
    }

    report.overflow_rows = sqlx::query_scalar("SELECT count(*) FROM telemetry_overflow")
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;

    tx.commit().await.map_err(db)?;

    // Rollups are a different problem with a different shape: no partitions to drop, so the window has to
    // be enforced with deletes. They run through the system context rather than the owner connection,
    // because `telemetry_rollup` has RLS forced on and `haven_system` is the role that may cross tenants.
    report.rollups_deleted = purge_rollups(pg, retention_days).await?;

    Ok(report)
}

/// Delete hourly rollups whose bucket is past the retention window, a batch at a time.
async fn purge_rollups(pg: &Pg, retention_days: i64) -> Result<i64, AppError> {
    let cutoff = OffsetDateTime::now_utc() - Duration::days(retention_days);
    let mut deleted = 0;

    for _ in 0..ROLLUP_BATCHES_PER_RUN {
        let mut tx = pg
            .scoped(&kernel::TenantCtx::system("telemetry-rollup-retention"))
            .await?;

        let batch = sqlx::query(
            "DELETE FROM telemetry_rollup
              WHERE (point_id, bucket) IN (
                    SELECT point_id, bucket FROM telemetry_rollup
                     WHERE bucket < $1
                     LIMIT $2
              )",
        )
        .bind(cutoff)
        .bind(ROLLUP_BATCH)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();

        tx.commit().await.map_err(db)?;

        deleted += i64::try_from(batch).unwrap_or(i64::MAX);
        if batch < u64::try_from(ROLLUP_BATCH).unwrap_or(u64::MAX) {
            break;
        }
    }

    Ok(deleted)
}

/// The pool a maintenance job needs, with the schema owner's rights: partitions are DDL, not tenant data.
pub fn pool(pg: &Pg) -> PgPool {
    pg.pool().clone()
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

fn current_month() -> Date {
    let today = OffsetDateTime::now_utc().date();
    Date::from_calendar_date(today.year(), today.month(), 1).expect("the 1st of a real month")
}

/// `2026-12` + 1 → `2027-01`. `time` has no month arithmetic for dates, and doing it with a day count is
/// the kind of "close enough" that lands a partition a day short in February.
fn shift_months(date: Date, months: i64) -> Date {
    let zero_based = i64::from(date.year()) * 12 + i64::from(u8::from(date.month())) - 1 + months;
    let year = i32::try_from(zero_based.div_euclid(12)).expect("year fits");
    let month = u8::try_from(zero_based.rem_euclid(12) + 1).expect("month fits");
    Date::from_calendar_date(year, Month::try_from(month).expect("1..=12"), 1).expect("the 1st")
}

fn partition_name(month_start: Date) -> String {
    format!(
        "telemetry_{:04}_{:02}",
        month_start.year(),
        u8::from(month_start.month())
    )
}

/// `telemetry_2026_11` → the first day of that month.
fn month_of(partition: &str) -> Option<Date> {
    let (year, month) = partition.strip_prefix("telemetry_")?.split_once('_')?;
    // Exactly `YYYY_MM`: `telemetry_27_01` parses as the year 27 and would be dropped by a job that only
    // meant to touch partitions this platform created. The digit count is the guard, not the parse.
    if year.len() != 4 || month.len() != 2 {
        return None;
    }
    if !year.bytes().all(|b| b.is_ascii_digit()) || !month.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: i32 = year.parse().ok()?;
    let month: u8 = month.parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    Date::from_calendar_date(year, Month::try_from(month).ok()?, 1).ok()
}

#[cfg(test)]
mod tests {
    use super::{Date, Month, month_of, shift_months};

    fn date(year: i32, month: u8) -> Date {
        Date::from_calendar_date(year, Month::try_from(month).unwrap(), 1).unwrap()
    }

    #[test]
    fn a_partition_name_carries_its_month() {
        let january = month_of("telemetry_2027_01").expect("month");
        assert_eq!(january.year(), 2027);
        assert_eq!(january.day(), 1);
        assert_eq!(u8::from(january.month()), 1);
    }

    #[test]
    fn names_that_are_not_months_are_not_touched() {
        for name in [
            "telemetry_overflow",
            "telemetry_2027_13",
            "telemetry_2027_00",
            "telemetry_27_01",
            "other_2027_01",
            "telemetry_2027_01_extra",
        ] {
            assert!(month_of(name).is_none(), "{name}");
        }
    }

    #[test]
    fn months_roll_over_the_year_boundary_in_both_directions() {
        assert_eq!(shift_months(date(2026, 12), 1), date(2027, 1));
        assert_eq!(shift_months(date(2026, 1), -1), date(2025, 12));
        assert_eq!(shift_months(date(2026, 11), 3), date(2027, 2));
        assert_eq!(shift_months(date(2024, 2), 12), date(2025, 2));
    }
}
