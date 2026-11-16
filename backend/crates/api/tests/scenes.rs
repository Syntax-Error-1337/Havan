//! Scenes and the automation hook: the two endpoints that change a home without the app.
//!
//! Neither had ever been driven end to end. Scene activation publishes a command per action and
//! reports what it dispatched — deliberately not what the device did, because a device that is
//! offline cannot do anything and lying about it would be worse than saying "sent". The automation
//! hook's URL *is* its credential, so a wrong token and a missing rule must be indistinguishable.

use api::{AppState, router};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

struct Fixture {
    base: String,
    admin: sqlx::PgPool,
    token: String,
    home_id: Uuid,
    device_id: Uuid,
}

async fn boot() -> Option<Fixture> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;
    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        "haven-api-scenes-test",
    )
    .ok()?;
    let state = AppState::new(cfg, pg, cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    let app = router(state.clone());
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    // A customer with a home, a claimed device and one point to switch.
    let email = format!("scene-{}@haven.dev", Uuid::now_v7());
    let tokens: serde_json::Value = client
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": "scene-Password-1" }))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let token = tokens["access"].as_str()?.to_string();

    let home: serde_json::Value = client
        .post(format!("{base}/v1/homes"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "name": "Scene flat", "timezone": "UTC" }))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let home_id: Uuid = home["id"].as_str()?.parse().ok()?;

    // Seeded with SQL, the way the bench does: the claim path is tested elsewhere, and what is
    // under test here is what a scene does with a device it did not create.
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db)
        .await
        .ok()?;
    let vendor_id = Uuid::now_v7();
    let product_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();
    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Scene Vendor')")
        .bind(vendor_id)
        .bind(format!("sc-{}", &vendor_id.simple().to_string()[24..]))
        .execute(&admin)
        .await
        .ok()?;
    let key = format!("scene_{}", &product_id.simple().to_string()[24..]);
    sqlx::query(
        "INSERT INTO products (id, vendor_id, product_key, name, category, thing_model)
         VALUES ($1, $2, $3, 'Scene Bulb', 'light', '{}'::jsonb)",
    )
    .bind(product_id)
    .bind(vendor_id)
    .bind(&key)
    .execute(&admin)
    .await
    .ok()?;
    sqlx::query(
        "INSERT INTO devices (id, vendor_id, product_id, product_key, home_id, name, serial,
                              secret_hash, state, category, online)
         VALUES ($1, $2, $3, $4, $5, 'Scene Bulb', $6, 'x', 'claimed', 'light', false)",
    )
    .bind(device_id)
    .bind(vendor_id)
    .bind(product_id)
    .bind(&key)
    .bind(home_id)
    .bind(format!("SC-{}", &device_id.simple().to_string()[24..]))
    .execute(&admin)
    .await
    .ok()?;
    sqlx::query(
        "INSERT INTO device_points (id, device_id, vendor_id, capability, slot, code, dp, kind,
                                    writable, history, value_bool)
         VALUES ($1, $2, $3, 'switch', 'power', 'switch.power', '1', 'bool', true, 'change', true)",
    )
    .bind(Uuid::now_v7())
    .bind(device_id)
    .bind(vendor_id)
    .execute(&admin)
    .await
    .ok()?;

    Some(Fixture {
        base,
        admin,
        token,
        home_id,
        device_id,
    })
}

#[tokio::test]
async fn activating_a_scene_dispatches_one_command_per_action() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/v1/homes/{}/scenes", f.base, f.home_id))
        .bearer_auth(&f.token)
        .json(&serde_json::json!({
            "name": "lights out",
            "actions": [{
                "type": "set_device",
                "device_id": f.device_id,
                "code": "switch.power",
                "value": false,
            }],
        }))
        .send()
        .await
        .expect("create")
        .json()
        .await
        .expect("scene");
    let scene_id = created["id"].as_str().expect("id").to_string();

    let activated = client
        .post(format!("{}/v1/scenes/{scene_id}/activate", f.base))
        .bearer_auth(&f.token)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("activate");
    assert_eq!(activated.status().as_u16(), 200);
    let body: serde_json::Value = activated.json().await.expect("activation");
    assert_eq!(body["dispatched"], 1, "one action, one command");
    assert_eq!(
        body["failed"].as_array().map(|a| a.len()),
        Some(0),
        "nothing refused"
    );

    // The command is real, and it is `queued` rather than `applied`: the device is offline, and a
    // scene that claimed success on an unreachable bulb would be the same lie presence had.
    let row: (String,) =
        sqlx::query_as("SELECT status FROM commands WHERE device_id = $1 ORDER BY created_at DESC")
            .bind(f.device_id)
            .fetch_one(&f.admin)
            .await
            .expect("command");
    assert_eq!(row.0, "queued", "a dispatched command waits for the device");
}

#[tokio::test]
async fn creating_a_scene_needs_a_manager_and_the_hook_needs_its_token() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let client = reqwest::Client::new();

    // A second account joined as a plain member cannot change the home for everybody.
    let email = format!("scene-member-{}@haven.dev", Uuid::now_v7());
    let tokens: serde_json::Value = client
        .post(format!("{}/v1/auth/register", f.base))
        .json(&serde_json::json!({ "email": email, "password": "scene-Password-1" }))
        .send()
        .await
        .expect("register")
        .json()
        .await
        .expect("tokens");
    let member = tokens["access"].as_str().expect("access").to_string();
    let _invited: serde_json::Value = client
        .post(format!("{}/v1/homes/{}/members", f.base, f.home_id))
        .bearer_auth(&f.token)
        .json(&serde_json::json!({ "email": email, "role": "member" }))
        .send()
        .await
        .expect("invite")
        .json()
        .await
        .expect("member");

    let refused = client
        .post(format!("{}/v1/homes/{}/scenes", f.base, f.home_id))
        .bearer_auth(&member)
        .json(&serde_json::json!({ "name": "nope", "actions": [] }))
        .send()
        .await
        .expect("create");
    assert_eq!(
        refused.status().as_u16(),
        403,
        "a plain member must not be able to create a scene"
    );

    // The hook: the token is the credential, and a bad one is not distinguishable from a missing rule.
    let rule: serde_json::Value = client
        .post(format!("{}/v1/homes/{}/automations", f.base, f.home_id))
        .bearer_auth(&f.token)
        .json(&serde_json::json!({
            "name": "hook rule",
            "definition": {
                "trigger": { "type": "webhook", "token": "right-token" },
                "actions": [{
                    "type": "set_device",
                    "device_id": f.device_id,
                    "code": "switch.power",
                    "value": true,
                }],
            },
        }))
        .send()
        .await
        .expect("automation")
        .json()
        .await
        .expect("rule");
    let rule_id = rule["id"].as_str().expect("id").to_string();

    let wrong = client
        .post(format!(
            "{}/v1/automations/{rule_id}/hook?token=nope",
            f.base
        ))
        .send()
        .await
        .expect("hook");
    assert_eq!(wrong.status().as_u16(), 404);

    let right = client
        .post(format!(
            "{}/v1/automations/{rule_id}/hook?token=right-token",
            f.base
        ))
        .send()
        .await
        .expect("hook");
    assert_eq!(right.status().as_u16(), 202, "armed, not run");
}
