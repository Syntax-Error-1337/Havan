//! Push tokens: how a phone tells the platform where to reach it, and how it takes that back.
//!
//! This file exists because of a defect only a request could find. The route was
//! `DELETE /notifications/tokens` and the handler extracted `Path<String>` — a parameter the route
//! did not have. The app called `DELETE /notifications/tokens/{token}`, got a 404, and the bare path
//! answered 500 with axum's own complaint in the body. Every unit test passed throughout: the app's
//! tests mock the transport, and nothing on the server ever called the route it served.
//!
//! So these tests call the URL the app calls, on a real socket, and then look in the database.

use api::{AppState, router};

async fn boot() -> Option<(String, AppState)> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        &format!(
            "{}-{}",
            "haven-api-notifications-test",
            uuid::Uuid::now_v7()
        ),
    )
    .ok()?;
    let state = AppState::new(cfg, pg, cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    let app = router(state.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    Some((format!("http://{addr}"), state))
}

async fn register(base: &str) -> String {
    let email = format!("push-{}@haven.dev", uuid::Uuid::now_v7());
    let res: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": "push-Password-1" }))
        .send()
        .await
        .expect("register")
        .json()
        .await
        .expect("tokens");
    res["access"].as_str().expect("access").to_string()
}

#[tokio::test]
async fn a_token_is_registered_by_the_path_the_app_deletes_it_with() {
    let Some((base, state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let token = register(&base).await;
    let client = reqwest::Client::new();
    let device_token = format!("tok-{}", uuid::Uuid::now_v7());

    let registered = client
        .post(format!("{base}/v1/notifications/tokens"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "platform": "ios",
            "token": device_token,
            "app_version": "1.0.0",
        }))
        .send()
        .await
        .expect("post");
    assert_eq!(registered.status().as_u16(), 204, "registering a token");

    // The URL the app actually uses on sign-out. This returned 404 before the route carried the
    // parameter its handler extracts.
    let removed = client
        .delete(format!("{base}/v1/notifications/tokens/{device_token}"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("delete");
    assert_eq!(
        removed.status().as_u16(),
        204,
        "deleting by path segment must answer 204, not 404 or 500"
    );

    // And it is gone: a 204 that removes nothing is the failure mode this asserts against.
    let rows: (i64,) = sqlx::query_as("SELECT count(*) FROM push_tokens WHERE token = $1")
        .bind(&device_token)
        .fetch_one(state.pg.pool())
        .await
        .expect("count");
    assert_eq!(rows.0, 0, "the token row should be gone after sign-out");

    // A second sign-out is not an error: the app may retry after a lost response.
    let again = client
        .delete(format!("{base}/v1/notifications/tokens/{device_token}"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("second delete");
    assert_eq!(again.status().as_u16(), 204, "removal is idempotent");
}

#[tokio::test]
async fn one_person_cannot_unregister_another_persons_phone() {
    let Some((base, state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let owner = register(&base).await;
    let stranger = register(&base).await;
    let client = reqwest::Client::new();
    let device_token = format!("tok-{}", uuid::Uuid::now_v7());

    client
        .post(format!("{base}/v1/notifications/tokens"))
        .bearer_auth(&owner)
        .json(&serde_json::json!({
            "platform": "android",
            "token": device_token,
            "app_version": "1.0.0",
        }))
        .send()
        .await
        .expect("post");

    // Removing is scoped to the caller's own tokens, so a stranger's delete is a no-op that the
    // owner's phone still receives alerts through.
    let attempt = client
        .delete(format!("{base}/v1/notifications/tokens/{device_token}"))
        .bearer_auth(&stranger)
        .send()
        .await
        .expect("delete");
    assert_eq!(attempt.status().as_u16(), 204, "the call itself is fine");

    let rows: (i64,) = sqlx::query_as("SELECT count(*) FROM push_tokens WHERE token = $1")
        .bind(&device_token)
        .fetch_one(state.pg.pool())
        .await
        .expect("count");
    assert_eq!(
        rows.0, 1,
        "another account must not be able to remove this token"
    );
}

#[tokio::test]
async fn an_alert_can_be_thrown_away_and_only_by_its_owner() {
    let Some((base, state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let client = reqwest::Client::new();
    let token = register(&base).await;

    // Who this token belongs to, and two alerts to work with.
    let me: serde_json::Value = client
        .get(format!("{base}/v1/me"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("me")
        .json()
        .await
        .expect("me json");
    let user_id = uuid::Uuid::parse_str(me["id"].as_str().expect("id")).expect("uuid");

    let mut ids = Vec::new();
    for title in ["Leak check", "Door opened"] {
        let id = uuid::Uuid::now_v7();
        sqlx::query("INSERT INTO notifications (id, user_id, kind, title, body) VALUES ($1, $2, 'alert', $3, '')")
            .bind(id)
            .bind(user_id)
            .bind(title)
            .execute(state.pg.pool())
            .await
            .expect("seed");
        ids.push(id);
    }

    let res = client
        .delete(format!("{base}/v1/notifications/{}", ids[0]))
        .bearer_auth(&token)
        .send()
        .await
        .expect("delete");
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.expect("json");
    assert_eq!(body["removed"], true);

    // The row is gone, and the other one is not: a dismiss that emptied the feed would pass a "removed: true" check.
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications WHERE id = ANY($1)")
        .bind(&ids)
        .fetch_one(state.pg.pool())
        .await
        .expect("count");
    assert_eq!(left, 1);

    // Somebody else's id is not a 403 and not a 500: it is a row that is not there.
    let second = register(&base).await;
    let res = client
        .delete(format!("{base}/v1/notifications/{}", ids[1]))
        .bearer_auth(&second)
        .send()
        .await
        .expect("delete other");
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.expect("json");
    assert_eq!(body["removed"], false);
    let still: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications WHERE id = $1")
        .bind(ids[1])
        .fetch_one(state.pg.pool())
        .await
        .expect("count");
    assert_eq!(still, 1);
}

#[tokio::test]
async fn a_person_can_see_which_phones_get_their_alerts_and_turn_them_off() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let token = register(&base).await;
    let client = reqwest::Client::new();
    let device_token = format!("tok-{}", uuid::Uuid::now_v7());

    let registered = client
        .post(format!("{base}/v1/notifications/tokens"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "platform": "ios", "token": device_token, "app_version": "0.1.0" }))
        .send()
        .await
        .expect("register");
    assert_eq!(
        registered.status().as_u16(),
        204,
        "registration answers with no content"
    );

    let listed: serde_json::Value = client
        .get(format!("{base}/v1/notifications/tokens"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("json");
    assert_eq!(listed["count"], 1);
    assert_eq!(listed["devices"][0]["platform"], "ios");
    // The delivery address must not travel back to the app: a screen has no use for it, and a log of it is a way to
    // push to somebody else's phone.
    assert!(
        listed["devices"][0].get("token").is_none(),
        "the token is not echoed to the client"
    );

    let off: serde_json::Value = client
        .delete(format!("{base}/v1/notifications/tokens"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("delete all")
        .json()
        .await
        .expect("json");
    assert_eq!(off["turned_off"], 1);

    let after: serde_json::Value = client
        .get(format!("{base}/v1/notifications/tokens"))
        .bearer_auth(&token)
        .send()
        .await
        .expect("list")
        .json()
        .await
        .expect("json");
    assert_eq!(after["count"], 0);
}
