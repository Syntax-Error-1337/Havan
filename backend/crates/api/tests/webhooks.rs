//! Webhooks: the registry, and the outbox that keeps delivery out of the request path.

use domain::webhook::{WebhookRepo, sign};
use kernel::ErrorCode;
use uuid::Uuid;

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
    let Some(f) = fixture().await else { return };
    let hooks = infra::PgWebhooks::new(f.pg.clone());
    let pool = admin(&f).await;

    let (_, secret) = hooks
        .create(
            f.vendor_id,
            "https://vendor.example.test/hook",
            vec!["licence.activated".into()],
        )
        .await
        .expect("create");

    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO webhook_outbox (id, vendor_id, event, payload) VALUES ($1, $2, 'licence.activated', $3)",
    )
    .bind(id)
    .bind(f.vendor_id)
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

#[tokio::test]
async fn a_failure_backs_off_and_is_bounded() {
    let Some(f) = fixture().await else { return };
    let hooks = infra::PgWebhooks::new(f.pg.clone());
    let pool = admin(&f).await;

    hooks
        .create(
            f.vendor_id,
            "https://vendor.example.test/hook",
            vec!["licence.activated".into()],
        )
        .await
        .expect("create");

    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO webhook_outbox (id, vendor_id, event, payload) VALUES ($1, $2, 'licence.activated', '{}'::jsonb)")
        .bind(id)
        .bind(f.vendor_id)
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
