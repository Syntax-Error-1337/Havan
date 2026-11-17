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

async fn fixture() -> Option<Fixture> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pool = PgPool::connect(&url).await.ok()?;
    let vendor = Uuid::now_v7();
    let home = Uuid::now_v7();

    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Run-now vendor')")
        .bind(vendor)
        .bind(format!("run{}", &vendor.simple().to_string()[..8]))
        .execute(&pool)
        .await
        .ok()?;
    sqlx::query("INSERT INTO homes (id, name, owner_id) VALUES ($1, 'Run-now home', $2)")
        .bind(home)
        .bind(Uuid::now_v7())
        .execute(&pool)
        .await
        .ok()?;

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
