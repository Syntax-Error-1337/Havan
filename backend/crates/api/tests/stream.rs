//! The live stream, end to end: a real socket, a real Redis publish.
//!
//! Runs only when `TEST_DATABASE_URL` and `TEST_REDIS_URL` are set, because it
//! needs the same infrastructure the process needs.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use api::{AppState, router};

async fn boot() -> Option<(String, AppState, String)> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    // Config::from_env needs the secrets, which CI provides.
    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let state = AppState::new(cfg, pg, cache);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    let app = router(state.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    Some((format!("http://{addr}"), state, addr.to_string()))
}

async fn post(url: &str, body: serde_json::Value) -> serde_json::Value {
    let res = reqwest::Client::new()
        .post(url)
        .json(&body)
        .send()
        .await
        .expect("request");
    res.json().await.expect("json")
}

#[tokio::test]
async fn a_subscriber_receives_an_event_for_its_home() {
    let Some((base, state, addr)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // A real account and a real home: membership is what the socket checks.
    let email = format!("stream-{}@haven.dev", uuid::Uuid::now_v7());
    let tokens = post(
        &format!("{base}/v1/auth/register"),
        serde_json::json!({ "email": email, "password": "correct horse battery" }),
    )
    .await;
    let access = tokens["access"].as_str().expect("access token").to_string();

    let home = reqwest::Client::new()
        .post(format!("{base}/v1/homes"))
        .bearer_auth(&access)
        .json(&serde_json::json!({ "name": "Stream flat", "timezone": "UTC" }))
        .send()
        .await
        .expect("home")
        .json::<serde_json::Value>()
        .await
        .expect("home json");
    let home_id = home["id"].as_str().expect("home id").to_string();

    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/v1/stream"))
        .await
        .expect("connect");

    socket
        .send(Message::Text(
            serde_json::json!({ "t": "auth", "token": access, "homes": [home_id] })
                .to_string()
                .into(),
        ))
        .await
        .expect("send auth");

    let ready = socket.next().await.expect("frame").expect("ok frame");
    assert!(
        ready.to_text().unwrap_or_default().contains("ready"),
        "expected a ready frame, got {ready:?}"
    );

    // The worker publishes here after ingest; the socket must forward it.
    let payload = serde_json::json!({
        "t": "point.changed",
        "home_id": home_id,
        "device_id": uuid::Uuid::now_v7(),
        "code": "switch.power",
        "value": true,
        "ts": 1
    });
    state
        .cache
        .publish_event(&format!("home:{home_id}"), &payload.to_string())
        .await
        .expect("publish");

    let event = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .expect("event within 5s")
        .expect("frame")
        .expect("ok");
    let text = event.to_text().unwrap_or_default().to_string();
    assert!(text.contains("point.changed"), "got {text}");
    assert!(text.contains("switch.power"), "got {text}");
}

#[tokio::test]
async fn a_socket_without_a_home_gets_no_access() {
    let Some((_base, _state, addr)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/v1/stream"))
        .await
        .expect("connect");

    // A home the caller cannot prove membership in.
    socket
        .send(Message::Text(
            serde_json::json!({
                "t": "auth",
                "token": "not-a-token",
                "homes": [uuid::Uuid::now_v7()]
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send auth");

    let frame = socket.next().await.expect("frame").expect("ok");
    let text = frame.to_text().unwrap_or_default().to_string();
    assert!(text.contains("invalid_token"), "got {text}");
}
