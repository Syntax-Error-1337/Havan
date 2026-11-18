//! Running a rule by hand.
//!
//! `POST /automations/{id}/run` arms the rule the same way the scheduler does, so the worker executes it
//! through one code path and the run lands in one log. Two properties matter and both are database
//! properties, so they are tested against the database:
//!
//! - the row becomes due **and** carries the manual stamp, because the stamp is how the audit trail
//!   distinguishes a test from a real firing (a hand-run schedule logged as `schedule` is exactly the
//!   reason this endpoint was held back for a year);
//! - a switched-off rule is refused rather than silently ignored.

use domain::automation::{AutomationRepo, Trigger};
use infra::repos::automation::PgAutomation;
use sqlx::PgPool;
use uuid::Uuid;

struct Fixture {
    pool: PgPool,
    repo: PgAutomation,
    home: Uuid,
}

/// `None` only when the environment is absent; a refused insert is a broken fixture and says so.
async fn fixture() -> Option<Fixture> {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        return None;
    };
    let pool = PgPool::connect(&url).await.expect("database");
    let vendor = Uuid::now_v7();
    let home = Uuid::now_v7();

    // One connection, and the row-level scope set on it: `vendors` is protected by RLS, and setting the scope on the
    // pool while inserting through the pool is a coin toss over which connection runs what.
    let mut tx = pool.begin().await.expect("seed transaction");
    sqlx::query("SELECT set_config('haven.vendor_id', $1, false)")
        .bind(vendor.to_string())
        .execute(&mut *tx)
        .await
        .expect("scope");
    // The tail of the uuid, not the head: a v7 uuid starts with a timestamp, so two fixtures made in the same
    // millisecond would share their first characters and collide on the slug.
    let tail = &vendor.simple().to_string()[24..];
    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Run-now vendor')")
        .bind(vendor)
        .bind(format!("run{tail}"))
        .execute(&mut *tx)
        .await
        .expect("vendor");
    // The owner has to exist: `homes.owner_id` gained a foreign key to `users` after this fixture was written, which is
    // one of the reasons the whole file had stopped running.
    let owner = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'Run-now owner')")
        .bind(owner)
        .bind(format!("run-{tail}@haven.dev"))
        .execute(&mut *tx)
        .await
        .expect("owner");
    sqlx::query("INSERT INTO homes (id, name, owner_id) VALUES ($1, 'Run-now home', $2)")
        .bind(home)
        .bind(owner)
        .execute(&mut *tx)
        .await
        .expect("home");
    tx.commit().await.expect("commit");

    Some(Fixture {
        repo: PgAutomation::new(infra::Pg::connect(&url, 2).await.ok()?),
        pool,
        home,
    })
}

async fn rule(f: &Fixture, every_secs: u32) -> Uuid {
    let def = domain::automation::AutomationDef {
        trigger: Trigger::Schedule { every_secs },
        conditions: vec![],
        actions: vec![],
    };
    f.repo
        .create(f.home, "A rule", &def, None)
        .await
        .expect("create")
        .id
}

#[tokio::test]
async fn running_now_makes_the_rule_due_and_stamps_it_as_manual() {
    let Some(f) = fixture().await else { return };
    let id = rule(&f, 86_400).await;

    // A daily rule is not due for a day; somebody pressing Run now makes it due in this instant.
    let due_now: bool = sqlx::query_scalar(
        "SELECT next_run_at IS NOT NULL AND next_run_at <= now() FROM automations WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .expect("due");
    assert!(!due_now, "a daily rule is not due immediately");

    assert!(f.repo.run_now(id).await.expect("run now"), "queued");

    let (due, stamped): (bool, bool) = sqlx::query_as(
        "SELECT next_run_at <= now(), run_requested_at IS NOT NULL FROM automations WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&f.pool)
    .await
    .expect("stamped");
    assert!(due, "the worker leases by next_run_at");
    assert!(stamped, "the trail has to be able to say 'manual'");

    f.repo.clear_run_request(id).await.expect("clear");
    let still_stamped: bool =
        sqlx::query_scalar("SELECT run_requested_at IS NOT NULL FROM automations WHERE id = $1")
            .bind(id)
            .fetch_one(&f.pool)
            .await
            .expect("cleared");
    assert!(!still_stamped, "one manual run, one stamp");

    sqlx::query("DELETE FROM automations WHERE id = $1")
        .bind(id)
        .execute(&f.pool)
        .await
        .ok();
}

#[tokio::test]
async fn a_rule_that_is_off_is_not_queued() {
    let Some(f) = fixture().await else { return };
    let id = rule(&f, 3_600).await;
    sqlx::query("UPDATE automations SET enabled = false WHERE id = $1")
        .bind(id)
        .execute(&f.pool)
        .await
        .expect("disable");

    // `false`, not an error: the route turns it into a 422 that says the rule is off. Silently doing
    // nothing would be the worst answer to a button somebody just pressed.
    assert!(!f.repo.run_now(id).await.expect("refused"));

    sqlx::query("DELETE FROM automations WHERE id = $1")
        .bind(id)
        .execute(&f.pool)
        .await
        .ok();
}

#[tokio::test]
async fn the_list_says_when_each_rule_last_ran() {
    let Some(f) = fixture().await else {
        eprintln!("skipping: TEST_DATABASE_URL not set");
        return;
    };
    let id = rule(&f, 3600).await;

    // A rule that has never fired says nothing about firing: the row is silent rather than claiming "never".
    let before = f.repo.list(f.home).await.expect("list");
    let listed = before
        .iter()
        .find(|a| a.id == id)
        .expect("the rule is listed");
    assert!(listed.last_run_at.is_none());

    // One run in the history, written the way the worker writes it.
    let started = time::OffsetDateTime::now_utc() - time::Duration::minutes(5);
    sqlx::query(
        "INSERT INTO automation_runs (id, automation_id, trigger_kind, status, started_at)
         VALUES ($1, $2, 'schedule', 'completed', $3)",
    )
    .bind(Uuid::now_v7())
    .bind(id)
    .bind(started)
    .execute(&f.pool)
    .await
    .expect("run row");

    // And the list carries it — one query, not one per row: this is the lateral join, and a test that only asserted
    // "some timestamp" would pass with a completely unwritten field.
    let after = f.repo.list(f.home).await.expect("list");
    let listed = after.iter().find(|a| a.id == id).expect("listed");
    let last = listed.last_run_at.expect("last run is filled in");
    assert!(
        (last - started).abs() < time::Duration::seconds(1),
        "the latest run's start, not some other row: {last} vs {started}"
    );
}
