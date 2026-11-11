//! Who owns an automation when two workers poll at the same instant.
//!
//! The scheduler's whole safety argument is one statement —
//! `UPDATE … WHERE id IN (SELECT … FOR UPDATE SKIP LOCKED)` — and until now it
//! was asserted in a comment. If it is wrong, two workers fire the same rule and
//! a lamp toggles twice; that is a bug nobody reports as a bug.
//!
//! Runs only when `TEST_DATABASE_URL` is set.

use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Duration;

use domain::automation::AutomationRepo;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

fn ids() -> Uuid {
    Uuid::now_v7()
}

/// `lease_due` is deliberately global — it leases *anything* due, for any home.
/// Two tests running in parallel therefore lease each other's rows, which is how
/// the first version of this file failed with an empty set. Serialise them.
fn exclusive() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Seed `count` scheduled rules that are already due.
async fn seed(pg: &sqlx::PgPool, count: usize) -> (Uuid, Vec<Uuid>) {
    let vendor_id = ids();
    let user_id = ids();
    let home_id = ids();

    let simple = vendor_id.simple().to_string();
    let short = &simple[simple.len() - 8..];

    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Lease Vendor')")
        .bind(vendor_id)
        .bind(format!("l-{short}"))
        .execute(pg)
        .await
        .expect("vendor");

    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("{user_id}@haven.test"))
        .execute(pg)
        .await
        .expect("user");

    sqlx::query("INSERT INTO homes (id, owner_id, name) VALUES ($1, $2, 'Lease flat')")
        .bind(home_id)
        .bind(user_id)
        .execute(pg)
        .await
        .expect("home");

    let mut created = Vec::new();
    for n in 0..count {
        let id = ids();
        let definition = serde_json::json!({
            "trigger": { "type": "schedule", "every_secs": 60 },
            "conditions": [],
            "actions": [{
                "type": "notify",
                "title": "Lease test",
                "body": format!("rule {n}")
            }]
        });

        sqlx::query(
            r#"INSERT INTO automations
                 (id, home_id, name, definition, next_run_at)
               VALUES ($1, $2, $3, $4, now() - interval '1 second')"#,
        )
        .bind(id)
        .bind(home_id)
        .bind(format!("rule {n}"))
        .bind(&definition)
        .execute(pg)
        .await
        .expect("automation");

        created.push(id);
    }

    (home_id, created)
}

async fn boot() -> Option<(sqlx::PgPool, infra::Pg, Uuid, Vec<Uuid>)> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let owner_db = std::env::var("MIGRATE_DATABASE_URL").unwrap_or_else(|_| db.clone());

    let pg = PgPoolOptions::new()
        .max_connections(12)
        .connect(&owner_db)
        .await
        .ok()?;

    // The leaser takes the first 50 due rules *in the database*, so leftovers
    // from earlier runs would crowd out this run's rules. These are ours by name.
    sqlx::query("DELETE FROM automations WHERE name LIKE 'rule %' OR name = 'broken rule'")
        .execute(&pg)
        .await
        .expect("clear previous runs");

    let (home_id, created) = seed(&pg, 20).await;
    let infra_pg = infra::Pg::connect(&db, 12).await.ok()?;

    Some((pg, infra_pg, home_id, created))
}

#[tokio::test]
async fn two_workers_never_lease_the_same_automation() {
    // Lock before booting: boot clears the previous run's rows, and a second
    // test doing that concurrently would delete the first one's seed.
    let _guard = exclusive().lock().await;
    let Some((_pg, infra_pg, _home, created)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL not set");
        return;
    };

    let repo = infra::PgAutomation::new(infra_pg.clone());
    let expected: HashSet<Uuid> = created.iter().copied().collect();

    // Two schedulers, eight rounds each, racing. Anything leased twice shows up
    // as a duplicate id across every round combined.
    let mut tasks = Vec::new();
    for worker in 0..2 {
        let repo = repo.clone();
        tasks.push(tokio::spawn(async move {
            let mut seen: Vec<Uuid> = Vec::new();
            for _ in 0..8 {
                let leased = repo
                    .lease_due(&format!("worker-{worker}"), 30)
                    .await
                    .expect("lease");
                seen.extend(leased.into_iter().map(|a| a.id));
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
            seen
        }));
    }

    let mut all: Vec<Uuid> = Vec::new();
    for task in tasks {
        all.extend(task.await.expect("task"));
    }

    let unique: HashSet<Uuid> = all.iter().copied().collect();
    assert_eq!(
        unique.len(),
        all.len(),
        "an automation was leased by two workers: {:?}",
        all.len() - unique.len()
    );

    // Every rule this test seeded was picked up. The leaser is global — anything
    // else that was due may join in, which is the point of it — so the claim is
    // "mine were all taken", not "only mine".
    assert!(
        expected.is_subset(&unique),
        "{} of the seeded rules were never leased",
        expected.difference(&unique).count()
    );
}

#[tokio::test]
async fn a_lease_expires_so_a_dead_worker_does_not_hold_a_rule_forever() {
    // Lock before booting: boot clears the previous run's rows, and a second
    // test doing that concurrently would delete the first one's seed.
    let _guard = exclusive().lock().await;
    let Some((pg, infra_pg, _home, created)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL not set");
        return;
    };

    let repo = infra::PgAutomation::new(infra_pg);
    let target = created[0];

    // Worker A takes the rule and "dies" holding it. Written directly, because
    // leasing it through the fleet first would race every other due rule in the
    // database — and the thing under test is the filter, not the ordering.
    // `next_run_at` is pushed far back so the rule sorts to the front of any
    // batch, whatever else is lying around.
    sqlx::query(
        r#"UPDATE automations
              SET lease_owner = 'worker-a',
                  lease_until = now() + interval '1 second',
                  next_run_at = now() - interval '7 days'
            WHERE id = $1"#,
    )
    .bind(target)
    .execute(&pg)
    .await
    .expect("plant a lease");

    // While the lease is live, a second worker must not be able to take it.
    let second = repo.lease_due("worker-b", 1).await.expect("lease");
    assert!(
        !second.iter().any(|a| a.id == target),
        "a live lease was stolen"
    );

    // Once it expires the rule is available again: an abandoned lease is a
    // delay, never a permanent loss.
    tokio::time::sleep(Duration::from_millis(1300)).await;
    let third = repo.lease_due("worker-b", 30).await.expect("lease");
    assert!(
        third.iter().any(|a| a.id == target),
        "an expired lease was never recovered"
    );

    let owner: Option<String> =
        sqlx::query_scalar("SELECT lease_owner FROM automations WHERE id = $1")
            .bind(target)
            .fetch_one(&pg)
            .await
            .expect("row");
    assert_eq!(owner.as_deref(), Some("worker-b"));
}

/// A rule whose definition no longer deserializes used to fail the whole batch,
/// which meant *no* automation in the database ran. It is now disabled by name.
#[tokio::test]
async fn one_corrupt_rule_does_not_stop_the_rest_of_the_fleet() {
    // Lock before booting: boot clears the previous run's rows, and a second
    // test doing that concurrently would delete the first one's seed.
    let _guard = exclusive().lock().await;
    let Some((pg, infra_pg, home, created)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL not set");
        return;
    };

    let bad = ids();
    sqlx::query(
        r#"INSERT INTO automations (id, home_id, name, definition, next_run_at)
           VALUES ($1, $2, 'broken rule',
                   '{"trigger":{"type":"schedule","every_secs":60},"actions":[{"type":"notify"}]}'::jsonb,
                   now() - interval '30 days')"#,
    )
    .bind(bad)
    .bind(home)
    .execute(&pg)
    .await
    .expect("broken automation");

    // Push the valid rules ahead of it so the batch is certain to contain both.
    sqlx::query(
        "UPDATE automations SET next_run_at = now() - interval '29 days' WHERE id = ANY($1)",
    )
    .bind(&created)
    .execute(&pg)
    .await
    .expect("age the good rules");

    let repo = infra::PgAutomation::new(infra_pg);
    let leased = repo
        .lease_due("worker-x", 30)
        .await
        .expect("a corrupt rule must not fail the batch");

    assert!(
        leased.iter().any(|a| a.id == created[0]),
        "the valid rules stopped running because of one bad row"
    );
    assert!(
        !leased.iter().any(|a| a.id == bad),
        "the corrupt rule was handed to the runner"
    );

    let enabled: bool = sqlx::query_scalar("SELECT enabled FROM automations WHERE id = $1")
        .bind(bad)
        .fetch_one(&pg)
        .await
        .expect("row");
    assert!(
        !enabled,
        "the corrupt rule should be switched off, not retried"
    );
}
