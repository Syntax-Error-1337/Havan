//! The order a person put their scenes in.
//!
//! The list was `ORDER BY created_at DESC`, so a scene's position was the moment it was created: the scene somebody uses
//! every day sat wherever it happened to land, and moving it meant deleting and rebuilding it — actions and all.

use api::{AppState, router};

async fn boot() -> Option<(String, AppState)> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        "haven-api-scene-order-test",
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
        .json(&serde_json::json!({ "email": email, "password": "scene-Order-Pass-1" }))
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

async fn home(base: &str, token: &str) -> uuid::Uuid {
    let body: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/v1/homes"))
        .bearer_auth(token)
        .json(&serde_json::json!({ "name": "Order test", "timezone": "UTC" }))
        .send()
        .await
        .expect("home")
        .json()
        .await
        .expect("home json");
    body["id"].as_str().expect("id").parse().expect("uuid")
}

#[tokio::test]
async fn a_scene_can_be_moved_to_where_the_person_wants_it() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let client = reqwest::Client::new();
    let owner = register(&base, &format!("order-{}@haven.dev", uuid::Uuid::now_v7())).await;
    let home_id = home(&base, &owner).await;

    // Three scenes, created oldest to newest: the list puts the newest on top.
    let mut ids = Vec::new();
    for name in ["Morning", "Evening", "Away"] {
        let scene: serde_json::Value = client
            .post(format!("{base}/v1/homes/{home_id}/scenes"))
            .bearer_auth(&owner)
            .json(&serde_json::json!({ "name": name, "actions": [] }))
            .send()
            .await
            .expect("scene")
            .json()
            .await
            .expect("json");
        ids.push(scene["id"].as_str().expect("id").to_string());
    }

    async fn names(base: &str, token: &str, home_id: &str) -> Vec<String> {
        let list: serde_json::Value = reqwest::Client::new()
            .get(format!("{base}/v1/homes/{home_id}/scenes"))
            .bearer_auth(token)
            .send()
            .await
            .expect("list")
            .json()
            .await
            .expect("json");
        list.as_array()
            .expect("array")
            .iter()
            .map(|s| s["name"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    assert_eq!(
        names(&base, &owner, &home_id.to_string()).await,
        ["Away", "Evening", "Morning"]
    );

    // Away is on top; walking it down should put Evening first.
    let moved = client
        .post(format!("{base}/v1/scenes/{}/move", ids[2]))
        .bearer_auth(&owner)
        .json(&serde_json::json!({ "up": false }))
        .send()
        .await
        .expect("move");
    assert_eq!(moved.status().as_u16(), 204);
    assert_eq!(
        names(&base, &owner, &home_id.to_string()).await,
        ["Evening", "Away", "Morning"]
    );

    // Moving it back up returns it to the top: the two moves are each other's undo.
    let back = client
        .post(format!("{base}/v1/scenes/{}/move", ids[2]))
        .bearer_auth(&owner)
        .json(&serde_json::json!({ "up": true }))
        .send()
        .await
        .expect("move up");
    assert_eq!(back.status().as_u16(), 204);
    assert_eq!(
        names(&base, &owner, &home_id.to_string()).await,
        ["Away", "Evening", "Morning"]
    );

    // A move at the edge is not an error and changes nothing - somebody's list may have changed under them.
    let edge = client
        .post(format!("{base}/v1/scenes/{}/move", ids[0]))
        .bearer_auth(&owner)
        .json(&serde_json::json!({ "up": false }))
        .send()
        .await
        .expect("move at the bottom");
    assert_eq!(edge.status().as_u16(), 204);
    assert_eq!(
        names(&base, &owner, &home_id.to_string()).await,
        ["Away", "Evening", "Morning"]
    );

    // And somebody outside the home cannot reorder it.
    let stranger = register(
        &base,
        &format!("order-x-{}@haven.dev", uuid::Uuid::now_v7()),
    )
    .await;
    let refused = client
        .post(format!("{base}/v1/scenes/{}/move", ids[0]))
        .bearer_auth(&stranger)
        .json(&serde_json::json!({ "up": true }))
        .send()
        .await
        .expect("stranger");
    assert_eq!(
        refused.status().as_u16(),
        404,
        "a scene you cannot see is not a scene you can move"
    );
}
