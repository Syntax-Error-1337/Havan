//! Two workers, one queue, one buzz.
//!
//! `undelivered()` used to select the queue and commit before the worker sent anything, so two
//! replicas both walked the same notifications and a phone buzzed twice for one alert. The row is
//! claimed with a lease now, the way the scheduler and the webhook outbox already were.
//!
//! The sweep is global, so these tests take turns — the same reason the presence tests do.

use domain::notification::{NotificationRepo, PushRepo};
use kernel::TenantCtx;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::{Mutex, MutexGuard, OnceCell};
use uuid::Uuid;

static CLAIM: OnceCell<Mutex<()>> = OnceCell::const_new();

async fn one_claim_at_a_time() -> MutexGuard<'static, ()> {
    CLAIM
        .get_or_init(|| async { Mutex::new(()) })
        .await
        .lock()
        .await
}

struct Fixture {
    pg: sqlx::PgPool,
    notifications: infra::PgNotifications,
    user_id: Uuid,
}

async fn boot() -> Option<Fixture> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db)
        .await
        .ok()?;

    let user_id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'Push Owner')")
        .bind(user_id)
        .bind(format!("push-{user_id}@example.test"))
        .execute(&admin)
        .await
        .ok()?;

    Some(Fixture {
        pg: admin,
        notifications: infra::PgNotifications::new(pg),
        user_id,
    })
}

impl Fixture {
    /// A queued alert, written the way the alert service writes one.
    async fn notification(&self, title: &str) -> Uuid {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO notifications (id, user_id, kind, title, body, data)
             VALUES ($1, $2, 'alert', $3, 'body', '{}'::jsonb)",
        )
        .bind(id)
        .bind(self.user_id)
        .bind(title)
        .execute(&self.pg)
        .await
        .expect("notification");
        id
    }
}

#[tokio::test]
async fn concurrent_workers_do_not_claim_the_same_notification() {
    let _only = one_claim_at_a_time().await;
    let Some(f) = boot().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };

    let first = f.notification("claim-a").await;
    let second = f.notification("claim-b").await;

    let (a, b) = tokio::join!(
        f.notifications.undelivered(500),
        f.notifications.undelivered(500)
    );
    let a = a.expect("first claim");
    let b = b.expect("second claim");

    let mine_a: Vec<Uuid> = a
        .iter()
        .map(|d| d.id)
        .filter(|id| [first, second].contains(id))
        .collect();
    let mine_b: Vec<Uuid> = b
        .iter()
        .map(|d| d.id)
        .filter(|id| [first, second].contains(id))
        .collect();

    assert!(
        !mine_a.is_empty() || !mine_b.is_empty(),
        "the seeded notifications should be claimable"
    );
    for id in &mine_a {
        assert!(
            !mine_b.contains(id),
            "one notification must not be claimed twice"
        );
    }

    // Leased: a third pass finds nothing of ours until the lease expires.
    let c = f.notifications.undelivered(500).await.expect("third claim");
    let mine_c: Vec<Uuid> = c
        .iter()
        .map(|d| d.id)
        .filter(|id| [first, second].contains(id))
        .collect();
    assert!(
        mine_c.is_empty(),
        "a claimed notification is not offered again"
    );
}

#[tokio::test]
async fn a_failed_push_schedules_its_own_retry() {
    let _only = one_claim_at_a_time().await;
    let Some(f) = boot().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };

    let id = f.notification("claim-fail").await;
    let claimed = f.notifications.undelivered(500).await.expect("claim");
    assert!(
        claimed.iter().any(|d| d.id == id),
        "the row should be claimable"
    );

    f.notifications
        .mark_attempt(id, "device token rejected")
        .await
        .expect("attempt");

    let (attempts, next_in, delivered): (i32, f64, bool) = sqlx::query_as(
        "SELECT delivery_attempts, extract(epoch from (next_attempt_at - now()))::float8,
                delivered_at IS NOT NULL
           FROM notifications WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&f.pg)
    .await
    .expect("row");

    assert_eq!(attempts, 1);
    assert!(
        next_in > 0.0,
        "the retry is scheduled in the future, not now"
    );
    assert!(
        !delivered,
        "one failure out of the budget does not settle the row"
    );
}

// Keeps the trait imports honest if the fixture stops using them directly.
#[allow(dead_code)]
fn _repos_are_reachable<N: NotificationRepo, P: PushRepo>() {}
#[allow(dead_code)]
fn _system_context(_: TenantCtx) {}
