//! Quiet hours.
//!
//! The app had no such control for one reason: the platform had nothing to enforce. A toggle over a phone that still
//! buzzes at 03:00 is worse than no toggle — it is a promise the product breaks while the customer sleeps.
//!
//! Two properties are checked here, and only one of them is about the API:
//!
//! 1. the window the API stores is the window it reads back, and the three shapes that cannot be honoured are refused;
//! 2. **enforcement is in the queue query**, so a held notification stays queued — it goes out when the window ends, and
//!    it does not consume a delivery attempt while it waits. That is the difference between "quiet" and "lost", and a
//!    Rust-side skip would have got it wrong in both directions.

use api::{AppState, router};
use domain::notification::PushRepo;
use uuid::Uuid;

struct Fixture {
    base: String,
    token: String,
    user_id: Uuid,
    pg: infra::Pg,
}

async fn boot() -> Option<Fixture> {
    let Ok(db) = std::env::var("TEST_DATABASE_URL") else {
        return None;
    };
    let Ok(redis) = std::env::var("TEST_REDIS_URL") else {
        return None;
    };

    let cfg = std::sync::Arc::new(kernel::Config::from_env().expect("config"));
    let pg = infra::Pg::connect(&db, 4).await.expect("database");
    let cache = infra::Cache::connect(&redis).await.expect("cache");
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        &format!("haven-api-quiet-{}", Uuid::now_v7()),
    )
    .expect("broker");
    let state = AppState::new(cfg, pg.clone(), cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, router(state)).await;
    });
    let base = format!("http://{addr}");

    let email = format!("quiet-{}@haven.dev", Uuid::now_v7());
    let tokens: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": "quiet-Password-1" }))
        .send()
        .await
        .expect("register")
        .json()
        .await
        .expect("json");
    let token = tokens["access"].as_str().expect("access").to_string();

    let me: serde_json::Value = reqwest::Client::new()
        .get(format!("{base}/v1/me"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("me")
        .json()
        .await
        .expect("me json");
    let user_id: Uuid = me["id"].as_str().expect("id").parse().expect("uuid");

    Some(Fixture {
        base,
        token,
        user_id,
        pg,
    })
}

async fn put(f: &Fixture, body: serde_json::Value) -> (u16, serde_json::Value) {
    let response = reqwest::Client::new()
        .put(format!("{}/v1/notifications/quiet-hours", f.base))
        .bearer_auth(&f.token)
        .json(&body)
        .send()
        .await
        .expect("put");
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    (
        status,
        serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)),
    )
}

/// A window that certainly contains the current UTC minute, expressed the way the API stores it.
async fn around_now(f: &Fixture) -> serde_json::Value {
    let mut tx =
        f.pg.scoped(&kernel::TenantCtx::system("quiet-test"))
            .await
            .expect("tx");
    let (from, to): (String, String) = sqlx::query_as(
        "SELECT to_char(now() - interval '2 hours', 'HH24:MI'), to_char(now() + interval '2 hours', 'HH24:MI')",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("clock");
    tx.commit().await.expect("commit");
    serde_json::json!({ "from": from, "to": to, "timezone": "UTC" })
}

async fn queue(f: &Fixture) -> usize {
    infra::PgNotifications::new(f.pg.clone())
        .undelivered(50)
        .await
        .expect("queue")
        .into_iter()
        .filter(|d| d.user_id == f.user_id)
        .count()
}

async fn seed_notification(f: &Fixture) {
    let mut tx =
        f.pg.scoped(&kernel::TenantCtx::system("quiet-test"))
            .await
            .expect("tx");
    // Backdated so it is the oldest row in the queue: `undelivered` orders by `created_at` and stops at its limit, and a
    // shared test database has thousands of older alerts. Newest-first would have made this test measure the limit.
    sqlx::query(
        "INSERT INTO notifications (id, user_id, kind, title, body, created_at)
         VALUES ($1, $2, 'alert', 'Front door', 'Motion', now() - interval '10 years')",
    )
    .bind(Uuid::now_v7())
    .bind(f.user_id)
    .execute(&mut *tx)
    .await
    .expect("notification");
    tx.commit().await.expect("commit");
}

#[tokio::test]
async fn the_window_you_set_is_the_window_you_read_back() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let (status, body) = put(
        &f,
        serde_json::json!({ "from": "22:00", "to": "07:00", "timezone": "Asia/Kolkata" }),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["from"], "22:00", "{body}");
    assert_eq!(body["to"], "07:00", "{body}");
    assert_eq!(body["timezone"], "Asia/Kolkata", "{body}");

    let read: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/v1/notifications/quiet-hours", f.base))
        .bearer_auth(&f.token)
        .send()
        .await
        .expect("get")
        .json()
        .await
        .expect("json");
    assert_eq!(read, body, "stored and read back must not drift");
}

#[tokio::test]
async fn a_window_that_cannot_be_honoured_is_refused() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // Not a time.
    assert_eq!(
        put(
            &f,
            serde_json::json!({ "from": "9:5", "to": "07:00", "timezone": "UTC" })
        )
        .await
        .0,
        422
    );
    // Same at both ends: all day, or never — the platform cannot guess which.
    assert_eq!(
        put(
            &f,
            serde_json::json!({ "from": "22:00", "to": "22:00", "timezone": "UTC" })
        )
        .await
        .0,
        422
    );
    // A zone Postgres cannot evaluate would silently never match, which is quiet hours that do nothing.
    assert_eq!(
        put(
            &f,
            serde_json::json!({ "from": "22:00", "to": "07:00", "timezone": "Mars/Olympus" })
        )
        .await
        .0,
        422
    );

    // And clearing works, because a toggle that cannot be turned off is not a toggle.
    let (status, body) = put(&f, serde_json::json!({})).await;
    assert_eq!(status, 200, "{body}");
    assert!(body["from"].is_null(), "{body}");
}

#[tokio::test]
async fn a_push_during_quiet_hours_is_held_not_lost() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    seed_notification(&f).await;

    // The pre-state is read with SQL, not by asking the queue: `undelivered` *claims* what it returns, so a first call to
    // prove the alert was deliverable would leave it on a five-minute lease and the quiet-hours check would then pass for
    // the wrong reason.
    put(&f, around_now(&f).await).await;
    assert_eq!(
        queue(&f).await,
        0,
        "inside the window the queue must not offer it for delivery"
    );

    // Held, not consumed: undelivered, unattempted, and still due — a claim would have pushed `next_attempt_at` forward
    // and burned the alert's chances.
    let (delivered, attempts, due): (bool, i32, bool) = sqlx::query_as(
        "SELECT delivered_at IS NOT NULL, delivery_attempts, next_attempt_at <= now()
           FROM notifications WHERE user_id = $1",
    )
    .bind(f.user_id)
    .fetch_one(f.pg.pool())
    .await
    .expect("row");
    assert!(!delivered, "quiet hours must not mark it delivered");
    assert_eq!(attempts, 0, "a held alert has not been attempted");
    assert!(due, "a held alert is still queued, not leased");

    put(&f, serde_json::json!({})).await;
    assert_eq!(
        queue(&f).await,
        1,
        "after the window the same alert is delivered — quiet is not the same as dropped"
    );
}
