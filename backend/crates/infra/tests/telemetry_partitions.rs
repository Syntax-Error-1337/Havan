//! The telemetry window, against a real database.
//!
//! The unit tests cover the month arithmetic. This covers the part that only a real Postgres can say:
//! a DEFAULT partition holding rows for a month that has no partition yet *blocks* creating that
//! partition, and the job is supposed to lift those rows out of the way and rescue them rather than fail.
//! That is not a hypothetical — migration 0003 started at the month it ran in, so the month before it is
//! stranded on any database that was migrated after it had already been reporting.

use infra::repos::telemetry_partitions::{DEFAULT_RETENTION_DAYS, maintain};
use time::{Date, Month, OffsetDateTime};
use uuid::Uuid;

fn month_offset(months: i32) -> Date {
    let today = OffsetDateTime::now_utc().date();
    let zero_based =
        i64::from(today.year()) * 12 + i64::from(u8::from(today.month())) - 1 + i64::from(months);
    Date::from_calendar_date(
        i32::try_from(zero_based.div_euclid(12)).unwrap(),
        Month::try_from(u8::try_from(zero_based.rem_euclid(12) + 1).unwrap()).unwrap(),
        1,
    )
    .unwrap()
}

fn partition_name(month: Date) -> String {
    format!(
        "telemetry_{:04}_{:02}",
        month.year(),
        u8::from(month.month())
    )
}

#[tokio::test]
async fn a_month_with_no_partition_is_created_and_its_rows_rescued() {
    // The schema owner, not the application role: creating and dropping a partition requires owning the
    // parent table, and the app role owns nothing by design (that is what makes RLS apply to it).
    let Ok(url) = std::env::var("TEST_MIGRATE_DATABASE_URL")
        .or_else(|_| std::env::var("MIGRATE_DATABASE_URL"))
        .or_else(|_| std::env::var("TEST_DATABASE_URL"))
    else {
        eprintln!("skipping: no test database configured");
        return;
    };
    let pg = infra::Pg::connect(&url, 2).await.expect("connect");

    // Next month, not this one: no other test writes readings for a month that has not started, so
    // dropping its partition here cannot take somebody else's rows with it.
    let month = month_offset(1);
    let next = month_offset(2);
    let name = partition_name(month);

    sqlx::query(&format!("DROP TABLE IF EXISTS \"{name}\""))
        .execute(pg.pool())
        .await
        .expect("drop the partition the migration made");

    let device = Uuid::now_v7();
    let vendor = Uuid::nil();
    let ts = month.midnight().assume_utc() + time::Duration::hours(6);

    // A reading for a month with nowhere to land goes to the DEFAULT partition.
    sqlx::query(
        "INSERT INTO telemetry_overflow (ts, device_id, point_id, vendor_id, num)
         VALUES ($1, $2, $3, $4, 21.5)",
    )
    .bind(ts)
    .bind(device)
    .bind(Uuid::now_v7())
    .bind(vendor)
    .execute(pg.pool())
    .await
    .expect("strand a reading in the overflow");

    let report = maintain(&pg, DEFAULT_RETENTION_DAYS)
        .await
        .expect("maintain");

    assert!(
        report.created.contains(&name),
        "the job creates the missing month: {:?}",
        report.created
    );
    assert!(
        report.rescued >= 1,
        "and it says how many readings it moved: {report:?}"
    );

    // The row is in the month partition, not in the overflow.
    let found: Vec<String> =
        sqlx::query_scalar("SELECT tableoid::regclass::text FROM telemetry WHERE device_id = $1")
            .bind(device)
            .fetch_all(pg.pool())
            .await
            .expect("read back");
    assert_eq!(found, vec![name.clone()], "the reading lives in {name}");

    let still_overflowed: i64 =
        sqlx::query_scalar("SELECT count(*) FROM telemetry_overflow WHERE device_id = $1")
            .bind(device)
            .fetch_one(pg.pool())
            .await
            .expect("count");
    assert_eq!(still_overflowed, 0);

    // Running it again is a no-op, which is what makes it safe on a schedule.
    let again = maintain(&pg, DEFAULT_RETENTION_DAYS)
        .await
        .expect("maintain again");
    assert!(
        !again.created.contains(&name),
        "a second run creates nothing: {:?}",
        again.created
    );
    assert_eq!(again.rescued, 0);

    // Leave the database as it was found: a partition for next month, and no probe rows.
    sqlx::query("DELETE FROM telemetry WHERE device_id = $1")
        .bind(device)
        .execute(pg.pool())
        .await
        .expect("clean up");
    let _ = next;
}
