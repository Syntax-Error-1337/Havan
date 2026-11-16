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
        "haven-api-notifications-test",
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
