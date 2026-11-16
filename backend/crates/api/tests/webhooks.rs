//! Webhooks: the registry, and the outbox that keeps delivery out of the request path.

use domain::webhook::{WebhookRepo, sign};
use kernel::ErrorCode;
use tokio::sync::{Mutex, MutexGuard, OnceCell};
use uuid::Uuid;

/// One delivery test at a time.
///
/// `due()` is the global queue — it claims any vendor's rows, by design — so two tests running in
/// parallel claim each other's fixtures and then assert about rows that are already leased. The
/// first version of these tests failed intermittently for exactly that reason.
static DELIVERY: OnceCell<Mutex<()>> = OnceCell::const_new();

async fn one_delivery_at_a_time() -> MutexGuard<'static, ()> {
    DELIVERY
        .get_or_init(|| async { Mutex::new(()) })
        .await
        .lock()
        .await
}

struct Fixture {
    db_url: String,
    pg: infra::Pg,
    vendor_id: Uuid,
}

async fn fixture() -> Option<Fixture> {
    let db_url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pg = infra::Pg::connect(&db_url, 4).await.ok()?;
    let vendor_id = Uuid::now_v7();

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .ok()?;
    sqlx::query("SET ROLE haven_system")
        .execute(&pool)
        .await
        .ok()?;
    sqlx::query("INSERT INTO vendors (id, name, slug) VALUES ($1, 'Hook Vendor', $2)")
        .bind(vendor_id)
        .bind(format!("hook-{}", &vendor_id.to_string()[20..32]))
        .execute(&pool)
        .await
        .ok()?;

    Some(Fixture {
        db_url,
        pg,
        vendor_id,
    })
}

async fn admin(f: &Fixture) -> sqlx::PgPool {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&f.db_url)
        .await
        .expect("pool");
    sqlx::query("SET ROLE haven_system")
        .execute(&pool)
        .await
        .expect("system role");
    pool
}

#[tokio::test]
async fn a_subscription_returns_its_secret_once_and_lists_afterwards() {
    let Some(f) = fixture().await else { return };
    let hooks = infra::PgWebhooks::new(f.pg.clone());

    let (id, secret) = hooks
        .create(
            f.vendor_id,
            "https://vendor.example.test/haven",
            vec!["licence.activated".into()],
        )
        .await
        .expect("create");

    assert!(secret.starts_with("whsec_"));
    assert!(secret.len() > 40);

    let listed = hooks.list(f.vendor_id).await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    assert_eq!(listed[0].events, vec!["licence.activated".to_string()]);
    assert!(listed[0].active);

    // Deleting twice: the second is a 404, not a silent success.
    hooks.delete(f.vendor_id, id).await.expect("delete");
    let err = hooks.delete(f.vendor_id, id).await.unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::NotFound);
}

#[tokio::test]
async fn a_plain_http_endpoint_is_refused_because_the_secret_travels_with_it() {
    let Some(f) = fixture().await else { return };
    let hooks = infra::PgWebhooks::new(f.pg.clone());

    let err = hooks
        .create(
            f.vendor_id,
            "http://vendor.example.test/hook",
            vec!["device.claimed".into()],
        )
        .await
        .unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);

    // Loopback is allowed: that is how a vendor tests the receiver locally.
    assert!(
        hooks
            .create(
                f.vendor_id,
                "http://127.0.0.1:9999/hook",
                vec!["device.claimed".into()]
            )
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn the_outbox_delivers_once_then_stops_being_due() {
    let _only = one_delivery_at_a_time().await;
    let Some(f) = fixture().await else { return };
    let hooks = infra::PgWebhooks::new(f.pg.clone());
    let pool = admin(&f).await;

    let (subscription, secret) = hooks
        .create(
            f.vendor_id,
            "https://vendor.example.test/hook",
            vec!["licence.activated".into()],
        )
        .await
        .expect("create");

    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO webhook_outbox (id, vendor_id, subscription_id, event, payload) \
         VALUES ($1, $2, $3, 'licence.activated', $4)",
    )
    .bind(id)
    .bind(f.vendor_id)
    .bind(subscription)
    .bind(serde_json::json!({ "serial": "SN-1" }))
    .execute(&pool)
    .await
    .expect("outbox");

    // `due` is the global queue and this database is shared by the suite, so a small limit means the
    // assertion depends on how many rows other tests left behind — which is how this failed once
    // twelve undelivered rows existed. Ask for enough that the test is about its own row.
    let due = hooks.due(500).await.expect("due");
    let mine = due.iter().find(|d| d.id == id).expect("our row");
    assert_eq!(mine.secret, secret);
    assert_eq!(mine.event, "licence.activated");
    assert_eq!(mine.payload["serial"], "SN-1");

    // The receiver would verify this.
    let sig = sign(&mine.secret, 1_700_000_000, b"{\"a\":1}");
    assert!(sig.starts_with("sha256="));

    hooks.mark_delivered(id).await.expect("delivered");
    let due = hooks.due(10).await.expect("due again");
    assert!(due.iter().all(|d| d.id != id), "a delivered row is not due");
}

/// Two endpoints, one event: both must be told.
///
/// The queue joined every row to every active subscription with a matching event and then marked
/// the row delivered after the first success, so a vendor with two hooks heard each event at exactly
/// one of them — and which one was down to join order. Delivery is per subscription now, and this
/// asserts that a single event produces a delivery for each endpoint, each carrying its own secret.
#[tokio::test]
async fn an_event_reaches_every_endpoint_that_asked_for_it() {
    let _only = one_delivery_at_a_time().await;
    let Some(f) = fixture().await else { return };
    let hooks = infra::PgWebhooks::new(f.pg.clone());
    let pool = admin(&f).await;

    let (first, first_secret) = hooks
        .create(
            f.vendor_id,
            "https://one.example.test/hook",
            vec!["licence.activated".into()],
        )
        .await
        .expect("first subscription");
    let (second, second_secret) = hooks
        .create(
            f.vendor_id,
            "https://two.example.test/hook",
            vec!["licence.activated".into()],
        )
        .await
        .expect("second subscription");

    // One row per subscription is what enqueue writes; write both, as a claim would.
    let mut rows = Vec::new();
    for subscription in [first, second] {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO webhook_outbox (id, vendor_id, subscription_id, event, payload) \
             VALUES ($1, $2, $3, 'licence.activated', '{\"serial\":\"SN-2\"}'::jsonb)",
        )
        .bind(id)
        .bind(f.vendor_id)
        .bind(subscription)
        .execute(&pool)
        .await
        .expect("outbox");
        rows.push(id);
    }

    let due = hooks.due(500).await.expect("due");
    let mine: Vec<_> = due.iter().filter(|d| rows.contains(&d.id)).collect();
    assert_eq!(
        mine.len(),
        2,
        "one delivery per subscription, not one per event"
    );

    let urls: Vec<&str> = mine.iter().map(|d| d.url.as_str()).collect();
    assert!(urls.contains(&"https://one.example.test/hook"));
    assert!(urls.contains(&"https://two.example.test/hook"));

    // Each delivery is signed with its own endpoint's secret; sharing one would let a leaked
    // secret from one vendor endpoint verify another's traffic.
    for delivery in &mine {
        let expected = if delivery.url.contains("one.") {
            &first_secret
        } else {
            &second_secret
        };
        assert_eq!(&delivery.secret, expected);
    }

    // Delivering to one must not silence the other. Both rows are leased by the claim above, so the
    // second is not offered again until its lease runs out — clear the lease as the clock would.
    let other = mine
        .iter()
        .find(|d| d.id != mine[0].id)
        .expect("a second delivery")
        .id;
    hooks.mark_delivered(mine[0].id).await.expect("delivered");
    sqlx::query("UPDATE webhook_outbox SET next_attempt_at = now() WHERE id = $1")
        .bind(other)
        .execute(&pool)
        .await
        .expect("clear the lease");

    let after = hooks.due(500).await.expect("due again");
    let still_owed: Vec<_> = after.iter().filter(|d| d.id == other).collect();
    assert_eq!(
        still_owed.len(),
        1,
        "the other endpoint is still owed the event"
    );
}

/// Two workers must not deliver the same event twice.
///
/// `due()` used to select with `FOR UPDATE SKIP LOCKED` and commit before sending, so the lock was
/// gone by the time the HTTP request left and two replicas could both claim the same row. The claim
/// is now a lease: this asserts that concurrent claims never overlap, and that a claimed row is not
/// offered again while the first worker is still delivering it.
#[tokio::test]
async fn concurrent_workers_do_not_claim_the_same_delivery() {
    let _only = one_delivery_at_a_time().await;
    let Some(f) = fixture().await else { return };
    let hooks = infra::PgWebhooks::new(f.pg.clone());
    let pool = admin(&f).await;

    let (subscription, _) = hooks
        .create(
            f.vendor_id,
            "https://lease.example.test/hook",
            vec!["licence.activated".into()],
        )
        .await
        .expect("create");

    let mut seeded = Vec::new();
    for _ in 0..6 {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO webhook_outbox (id, vendor_id, subscription_id, event, payload) \
             VALUES ($1, $2, $3, 'licence.activated', '{}'::jsonb)",
        )
        .bind(id)
        .bind(f.vendor_id)
        .bind(subscription)
        .execute(&pool)
        .await
        .expect("outbox");
        seeded.push(id);
    }

    let (a, b) = tokio::join!(hooks.due(500), hooks.due(500));
    let a = a.expect("first claim");
    let b = b.expect("second claim");

    let mine_a: Vec<Uuid> = a
        .iter()
        .map(|d| d.id)
        .filter(|id| seeded.contains(id))
        .collect();
    let mine_b: Vec<Uuid> = b
        .iter()
        .map(|d| d.id)
        .filter(|id| seeded.contains(id))
        .collect();

    assert!(
        !mine_a.is_empty() || !mine_b.is_empty(),
        "the seeded rows should be claimable"
    );
    for id in &mine_a {
        assert!(!mine_b.contains(id), "one row must not be claimed twice");
    }

    // Leased: a third claim finds nothing of ours until the lease expires.
    let c = hooks.due(500).await.expect("third claim");
    let mine_c: Vec<Uuid> = c
        .iter()
        .map(|d| d.id)
        .filter(|id| seeded.contains(id))
        .collect();
    assert!(
        mine_c.is_empty(),
        "a claimed row is not offered again while leased"
    );
}

#[tokio::test]
async fn a_failure_backs_off_and_is_bounded() {
    let _only = one_delivery_at_a_time().await;
    let Some(f) = fixture().await else { return };
    let hooks = infra::PgWebhooks::new(f.pg.clone());
    let pool = admin(&f).await;

    let (subscription, _) = hooks
        .create(
            f.vendor_id,
            "https://vendor.example.test/hook",
            vec!["licence.activated".into()],
        )
        .await
        .expect("create");

    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO webhook_outbox (id, vendor_id, subscription_id, event, payload) \
         VALUES ($1, $2, $3, 'licence.activated', '{}'::jsonb)",
    )
    .bind(id)
    .bind(f.vendor_id)
    .bind(subscription)
    .execute(&pool)
    .await
    .expect("outbox");

    hooks
        .mark_failed(id, "endpoint answered 503")
        .await
        .expect("fail");

    let (attempts, error, next_in): (i32, Option<String>, f64) = sqlx::query_as(
        "SELECT attempts, last_error, extract(epoch from (next_attempt_at - now()))::float8 \
           FROM webhook_outbox WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .expect("row");

    assert_eq!(attempts, 1);
    assert_eq!(error.as_deref(), Some("endpoint answered 503"));
    assert!(next_in > 0.0, "the retry must be scheduled in the future");

    // Not due yet.
    let due = hooks.due(10).await.expect("due");
    assert!(due.iter().all(|d| d.id != id));
}
