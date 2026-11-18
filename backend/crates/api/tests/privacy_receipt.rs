//! The receipt for a data export.
//!
//! "A copy of your data" streamed a bundle and left no trace, so the screen that offers it could not say whether a
//! request had ever been answered. A subject-access request is answered by a record as much as by a file. These tests
//! drive it on a socket: export, then read the receipt back, and check that a second account cannot see somebody else's.

use api::{AppState, router};

async fn boot() -> Option<(String, AppState)> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        "haven-api-receipts-test",
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

async fn register(base: &str, email: &str) -> String {
    reqwest::Client::new()
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": "receipt-Password-1" }))
        .send()
        .await
        .expect("register")
        .json::<serde_json::Value>()
        .await
        .expect("tokens")["access"]
        .as_str()
        .expect("access")
        .to_string()
}

#[tokio::test]
async fn an_export_leaves_a_receipt_and_only_its_owner_can_read_it() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let client = reqwest::Client::new();
    let me = register(
        &base,
        &format!("receipt-{}@haven.dev", uuid::Uuid::now_v7()),
    )
    .await;
    let other = register(
        &base,
        &format!("receipt-{}@haven.dev", uuid::Uuid::now_v7()),
    )
    .await;

    // Nothing has been exported yet, and the screen has to be able to say so rather than show an empty date.
    let empty: serde_json::Value = client
        .get(format!("{base}/v1/me/exports"))
        .bearer_auth(&me)
        .send()
        .await
        .expect("receipts")
        .json()
        .await
        .expect("json");
    assert_eq!(empty["count"], 0);

    let export = client
        .get(format!("{base}/v1/me/export"))
        .bearer_auth(&me)
        .send()
        .await
        .expect("export");
    assert_eq!(export.status().as_u16(), 200);
    assert!(
        export
            .headers()
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("attachment")),
        "the export is still a file to save"
    );
    let body_len = export.bytes().await.expect("bytes").len() as i64;

    let after: serde_json::Value = client
        .get(format!("{base}/v1/me/exports"))
        .bearer_auth(&me)
        .send()
        .await
        .expect("receipts")
        .json()
        .await
        .expect("json");
    assert_eq!(after["count"], 1, "one export, one receipt: {after}");
    assert_eq!(
        after["exports"][0]["bytes"].as_i64(),
        Some(body_len),
        "the receipt measures what was actually handed over"
    );
    assert!(
        after["exports"][0]["created_at"].is_string(),
        "a receipt without a date answers nothing: {after}"
    );

    // Somebody else's receipts are not mine to read, and mine are not theirs.
    let theirs: serde_json::Value = client
        .get(format!("{base}/v1/me/exports"))
        .bearer_auth(&other)
        .send()
        .await
        .expect("receipts")
        .json()
        .await
        .expect("json");
    assert_eq!(theirs["count"], 0);
}
