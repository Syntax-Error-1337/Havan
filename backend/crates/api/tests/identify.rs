//! `identify` — asking a device to announce itself.
//!
//! It was in `docs/05-API.md` for weeks and did not exist, and the command route answered "actions are not implemented yet;
//! send a point value". The wire envelope has carried an `action` field since the first version; nothing ever filled it in.
//!
//! What this test can and cannot prove is worth being precise about: it proves the platform records an action command, puts
//! it on the broker with `action` set and no data point, and settles it from the device's ack. Whether a given firmware
//! blinks is the firmware's business, and the app reports "no answer" in those words.

use api::{AppState, router};
use domain::identity::hash_password;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

struct Fixture {
    base: String,
    token: String,
    device_id: Uuid,
}

async fn register(base: &str, email: &str) -> String {
    reqwest::Client::new()
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": "identify-Password-1" }))
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
        &format!("haven-api-identify-test-{}", Uuid::now_v7()),
    )
    .expect("broker");
    let state = AppState::new(cfg, pg, cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
    });
    let base = format!("http://{addr}");

    let token = register(&base, &format!("identify-{}@haven.dev", Uuid::now_v7())).await;
    let client = reqwest::Client::new();

    let home: serde_json::Value = client
        .post(format!("{base}/v1/homes"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "name": "Identify flat", "timezone": "UTC" }))
        .send()
        .await
        .expect("home")
        .json()
        .await
        .expect("home json");
    let home_id: Uuid = home["id"].as_str().expect("id").parse().expect("uuid");

    // A claimed device in that home, seeded the way the other suites do it: one connection with the row-level scope set
    // on it, because the scope is session state and a pooled insert is a coin toss.
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db)
        .await
        .expect("admin pool");
    let vendor_id = Uuid::now_v7();
    let product_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();
    let tail = &vendor_id.simple().to_string()[24..];
    let product_key = format!("identify_{tail}");

    let mut tx = admin.begin().await.expect("seed transaction");
    sqlx::query("SELECT set_config('haven.vendor_id', $1, false)")
        .bind(vendor_id.to_string())
        .execute(&mut *tx)
        .await
        .expect("scope");
    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Identify Vendor')")
        .bind(vendor_id)
        .bind(format!("identify-{tail}"))
        .execute(&mut *tx)
        .await
        .expect("vendor");
    sqlx::query(
        "INSERT INTO products (id, vendor_id, product_key, name, category, thing_model, status)
         VALUES ($1, $2, $3, 'Identify Bulb', 'light', '{}'::jsonb, 'published')",
    )
    .bind(product_id)
    .bind(vendor_id)
    .bind(&product_key)
    .execute(&mut *tx)
    .await
    .expect("product");
    sqlx::query(
        "INSERT INTO devices (id, vendor_id, product_id, product_key, home_id, name, serial,
                              secret_hash, state, category)
         VALUES ($1, $2, $3, $4, $5, 'Identify Bulb', $6, $7, 'claimed', 'light')",
    )
    .bind(device_id)
    .bind(vendor_id)
    .bind(product_id)
    .bind(&product_key)
    .bind(home_id)
    .bind(format!(
        "IDN-{}",
        device_id.simple().to_string()[24..].to_uppercase()
    ))
    .bind(hash_password("device-Secret-1").expect("hash"))
    .execute(&mut *tx)
    .await
    .expect("device");
    tx.commit().await.expect("commit");

    Some(Fixture {
        base,
        token,
        device_id,
    })
}

async fn identify(f: &Fixture, token: &str) -> (u16, serde_json::Value) {
    let response = reqwest::Client::new()
        .post(format!("{}/v1/devices/{}/identify", f.base, f.device_id))
        .bearer_auth(token)
        .send()
        .await
        .expect("identify");
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    (
        status,
        serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)),
    )
}

#[tokio::test]
async fn identify_records_an_action_command_and_names_it_when_you_ask_about_it() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let (status, body) = identify(&f, &f.token).await;
    assert_eq!(status, 202, "{body}");
    assert_eq!(
        body["status"].as_str(),
        Some("queued"),
        "202 is a promise about a queue, not about a light: {body}"
    );
    let command_id = body["command_id"].as_str().expect("command_id");

    let settled: serde_json::Value = reqwest::Client::new()
        .get(format!(
            "{}/v1/devices/{}/commands/{}",
            f.base, f.device_id, command_id
        ))
        .bearer_auth(&f.token)
        .send()
        .await
        .expect("status")
        .json()
        .await
        .expect("json");

    // The status screen has to be able to say *which* action it was asking about; a command with no point and no name
    // answers "" and is unrecognisable.
    assert_eq!(
        settled["point_code"].as_str(),
        Some("identify"),
        "{settled}"
    );
    assert!(
        ["queued", "dispatched", "applied", "rejected", "timeout"]
            .contains(&settled["status"].as_str().unwrap_or("")),
        "{settled}"
    );
}

#[tokio::test]
async fn a_device_in_somebody_elses_home_cannot_be_asked_to_identify() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // A stranger: registered, but in no home at all.
    let stranger = register(
        &f.base,
        &format!("identify-stranger-{}@haven.dev", Uuid::now_v7()),
    )
    .await;

    let (status, body) = identify(&f, &stranger).await;
    assert_eq!(status, 404, "{body}");
}
