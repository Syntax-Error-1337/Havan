//! The home screen's one request, and the room edits behind the settings screen.
//!
//! `/homes/{id}/dashboard` composes five services. The thing worth testing is not
//! that it returns JSON — it is that it returns *this user's* JSON: a composer that
//! forgets one tenancy check leaks a stranger's home through the one endpoint every
//! screen calls.

use api::{AppState, router};

async fn boot() -> Option<(String, AppState)> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        "haven-api-dashboard-test",
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

struct Caller {
    token: String,
}

async fn register(base: &str) -> Caller {
    let email = format!("dash-{}@haven.dev", uuid::Uuid::now_v7());
    let res: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": "dashboard-Password-1" }))
        .send()
        .await
        .expect("register")
        .json()
        .await
        .expect("tokens");
    Caller {
        token: res["access"].as_str().expect("access").to_string(),
    }
}

impl Caller {
    async fn get(&self, base: &str, path: &str) -> (u16, serde_json::Value) {
        let res = reqwest::Client::new()
            .get(format!("{base}/v1{path}"))
            .bearer_auth(&self.token)
            .send()
            .await
            .expect("get");
        let status = res.status().as_u16();
        (status, res.json().await.unwrap_or(serde_json::Value::Null))
    }

    async fn send(
        &self,
        base: &str,
        method: &str,
        path: &str,
        body: serde_json::Value,
    ) -> (u16, serde_json::Value) {
        let method = reqwest::Method::from_bytes(method.as_bytes()).expect("method");
        let res = reqwest::Client::new()
            .request(method, format!("{base}/v1{path}"))
            .bearer_auth(&self.token)
            .json(&body)
            .send()
            .await
            .expect("send");
        let status = res.status().as_u16();
        (status, res.json().await.unwrap_or(serde_json::Value::Null))
    }
}

#[tokio::test]
async fn the_dashboard_is_one_call_and_only_this_home() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let me = register(&base).await;
    let (status, home) = me
        .send(
            &base,
            "POST",
            "/homes",
            serde_json::json!({ "name": "Dash flat" }),
        )
        .await;
    assert_eq!(status, 201);
    let home_id = home["id"].as_str().expect("home id").to_string();

    let (status, room) = me
        .send(
            &base,
            "POST",
            &format!("/homes/{home_id}/rooms"),
            serde_json::json!({ "name": "Kitchen" }),
        )
        .await;
    assert_eq!(status, 201, "room: {room}");
    let room_id = room["id"].as_str().expect("room id").to_string();

    let (status, dash) = me.get(&base, &format!("/homes/{home_id}/dashboard")).await;
    assert_eq!(status, 200, "dashboard: {dash}");

    // The whole screen arrives at once, in its five parts.
    assert_eq!(dash["home"]["id"], home_id.as_str());
    assert_eq!(dash["rooms"].as_array().map(Vec::len), Some(1));
    assert_eq!(dash["rooms"][0]["name"], "Kitchen");
    assert!(dash["devices"].is_array());
    assert!(dash["scenes"].is_array());
    assert_eq!(dash["unread_count"], 0);

    // A stranger gets nothing — not an empty dashboard, a 404.
    let other = register(&base).await;
    let (status, _) = other
        .get(&base, &format!("/homes/{home_id}/dashboard"))
        .await;
    assert_eq!(status, 404, "another account must not read this dashboard");

    // Renaming needs membership too, and then it sticks.
    let (status, _) = other
        .send(
            &base,
            "PATCH",
            &format!("/rooms/{room_id}?home_id={home_id}"),
            serde_json::json!({ "name": "Stolen" }),
        )
        .await;
    assert_eq!(status, 404, "a stranger cannot rename a room");

    let (status, _) = me
        .send(
            &base,
            "PATCH",
            &format!("/rooms/{room_id}?home_id={home_id}"),
            serde_json::json!({ "name": "  Pantry  " }),
        )
        .await;
    assert_eq!(status, 204, "rename");

    let (_, dash) = me.get(&base, &format!("/homes/{home_id}/dashboard")).await;
    assert_eq!(
        dash["rooms"][0]["name"], "Pantry",
        "the name is trimmed and current"
    );

    // An empty rename is a validation error, not a room called "  ".
    let (status, _) = me
        .send(
            &base,
            "PATCH",
            &format!("/rooms/{room_id}?home_id={home_id}"),
            serde_json::json!({ "name": "   " }),
        )
        .await;
    assert_eq!(status, 422, "a room needs a name");
}

#[tokio::test]
async fn health_reports_the_broker() {
    let Some((base, _state)) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let res = reqwest::Client::new()
        .get(format!("{base}/healthz"))
        .send()
        .await
        .expect("health");
    let body: serde_json::Value = res.json().await.expect("json");

    // The road map promised db, redis *and* mqtt; a broker the API cannot reach is
    // a command pipeline that accepts work it cannot deliver.
    assert!(body["components"]["db"].is_boolean());
    assert!(body["components"]["redis"].is_boolean());
    assert!(
        body["components"]["mqtt"].is_boolean(),
        "health must report the broker: {body}"
    );
}
