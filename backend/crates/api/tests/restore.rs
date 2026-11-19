//! Undoing a removal.
//!
//! Removing a device from a home was one-way: the row survives as `released`, but the only route back was pairing it
//! again — factory reset, QR code, ticket. For the one destructive action in the app that is a bad trade, and the confirm
//! dialog had to admit it ("to use it again, pair it again").
//!
//! What makes an undo safe is that it cannot be replayed: a release is single-use, and it expires. Both are checked here,
//! because both are the difference between an undo and a way to move somebody else's device into your home.

use api::{AppState, router};
use domain::identity::hash_password;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

struct Fixture {
    base: String,
    admin: sqlx::PgPool,
    vendor_id: Uuid,
    home_id: Uuid,
    device_id: Uuid,
    room_id: Uuid,
    owner: String,
    member: String,
}

struct Seeded {
    owner_email: String,
    member_email: String,
}

async fn register(base: &str, email: &str) -> String {
    reqwest::Client::new()
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": "restore-Password-1" }))
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
        &format!("haven-api-restore-test-{}", Uuid::now_v7()),
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

    // Two people in one home, through the public API: the admin who removes, and the member who may not undo it.
    let stamp = Uuid::now_v7();
    let seeded = Seeded {
        owner_email: format!("restore-owner-{stamp}@haven.dev"),
        member_email: format!("restore-member-{stamp}@haven.dev"),
    };
    let owner = register(&base, &seeded.owner_email).await;
    let member = register(&base, &seeded.member_email).await;

    let home: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/v1/homes"))
        .bearer_auth(&owner)
        .json(&serde_json::json!({ "name": "Restore flat", "timezone": "UTC" }))
        .send()
        .await
        .expect("home")
        .json()
        .await
        .expect("home json");
    let home_id: Uuid = home["id"].as_str().expect("id").parse().expect("uuid");

    // The member joins by invitation — the only way into somebody else's home.
    let joined: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/v1/homes/{home_id}/members"))
        .bearer_auth(&owner)
        .json(&serde_json::json!({ "email": seeded.member_email, "role": "member" }))
        .send()
        .await
        .expect("invite")
        .json()
        .await
        .expect("json");
    assert_eq!(joined["outcome"], "added", "{joined}");

    let room: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/v1/homes/{home_id}/rooms"))
        .bearer_auth(&owner)
        .json(&serde_json::json!({ "name": "Study" }))
        .send()
        .await
        .expect("room")
        .json()
        .await
        .expect("room json");
    let room_id: Uuid = room["id"].as_str().expect("room id").parse().expect("uuid");

    // A claimed device in that home and room, seeded the way the other suites do it: one connection, with the row-level
    // scope set on it.
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db)
        .await
        .expect("admin pool");
    let vendor_id = Uuid::now_v7();
    let product_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();
    let tail = &vendor_id.simple().to_string()[24..];
    let product_key = format!("restore_{tail}");

    let mut tx = admin.begin().await.expect("seed transaction");
    sqlx::query("SELECT set_config('haven.vendor_id', $1, false)")
        .bind(vendor_id.to_string())
        .execute(&mut *tx)
        .await
        .expect("scope");
    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Restore Vendor')")
        .bind(vendor_id)
        .bind(format!("restore-{tail}"))
        .execute(&mut *tx)
        .await
        .expect("vendor");
    sqlx::query(
        "INSERT INTO products (id, vendor_id, product_key, name, category, thing_model, status)
         VALUES ($1, $2, $3, 'Restore Bulb', 'light', '{}'::jsonb, 'published')",
    )
    .bind(product_id)
    .bind(vendor_id)
    .bind(&product_key)
    .execute(&mut *tx)
    .await
    .expect("product");
    sqlx::query(
        "INSERT INTO devices (id, vendor_id, product_id, product_key, home_id, room_id, name, serial,
                              secret_hash, state, category)
         VALUES ($1, $2, $3, $4, $5, $6, 'Restore Bulb', $7, $8, 'claimed', 'light')",
    )
    .bind(device_id)
    .bind(vendor_id)
    .bind(product_id)
    .bind(&product_key)
    .bind(home_id)
    .bind(room_id)
    .bind(format!(
        "RST-{}",
        device_id.simple().to_string()[24..].to_uppercase()
    ))
    .bind(hash_password("device-Secret-1").expect("hash"))
    .execute(&mut *tx)
    .await
    .expect("device");
    tx.commit().await.expect("commit");

    // The registrations above produced the member's token; sign in again for a fresh one after the invite.
    let member_token = reqwest::Client::new()
        .post(format!("{base}/v1/auth/login"))
        .json(
            &serde_json::json!({ "email": seeded.member_email, "password": "restore-Password-1" }),
        )
        .send()
        .await
        .expect("login")
        .json::<serde_json::Value>()
        .await
        .expect("json")["access"]
        .as_str()
        .expect("access")
        .to_string();
    let _ = member_token;

    Some(Fixture {
        base,
        admin,
        vendor_id,
        home_id,
        device_id,
        room_id,
        owner,
        member,
    })
}

async fn remove(f: &Fixture, token: &str) -> u16 {
    reqwest::Client::new()
        .delete(format!("{}/v1/devices/{}", f.base, f.device_id))
        .bearer_auth(token)
        .send()
        .await
        .expect("remove")
        .status()
        .as_u16()
}

async fn restore(f: &Fixture, token: &str) -> (u16, serde_json::Value) {
    let response = reqwest::Client::new()
        .post(format!("{}/v1/devices/{}/restore", f.base, f.device_id))
        .bearer_auth(token)
        .send()
        .await
        .expect("restore");
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    (
        status,
        serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text)),
    )
}

#[tokio::test]
async fn a_removal_can_be_undone_and_the_device_comes_back_where_it_was() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    assert_eq!(remove(&f, &f.owner).await, 204);

    let (status, body) = restore(&f, &f.owner).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        body["home_id"].as_str().expect("home_id"),
        f.home_id.to_string(),
        "the answer names the home it went back to"
    );

    let device: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/v1/devices/{}", f.base, f.device_id))
        .bearer_auth(&f.owner)
        .send()
        .await
        .expect("get")
        .json()
        .await
        .expect("json");
    assert_eq!(
        device["room_id"].as_str(),
        Some(f.room_id.to_string().as_str()),
        "it came back to the room it was taken from, not to the bottom of the list"
    );
}

#[tokio::test]
async fn a_release_is_single_use() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    assert_eq!(remove(&f, &f.owner).await, 204);
    assert_eq!(restore(&f, &f.owner).await.0, 200);

    // Pressing undo twice must not move a device out of wherever it has been since.
    let (status, body) = restore(&f, &f.owner).await;
    assert_eq!(status, 409, "{body}");
}

#[tokio::test]
async fn the_undo_expires() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    assert_eq!(remove(&f, &f.owner).await, 204);

    // Age the release rather than waiting ten minutes: the window is the property, not the clock.
    let mut tx = f.admin.begin().await.expect("tx");
    sqlx::query("SELECT set_config('haven.vendor_id', $1, false)")
        .bind(f.vendor_id.to_string())
        .execute(&mut *tx)
        .await
        .expect("scope");
    sqlx::query("UPDATE device_releases SET released_at = now() - interval '11 minutes' WHERE device_id = $1")
        .bind(f.device_id)
        .execute(&mut *tx)
        .await
        .expect("age");
    tx.commit().await.expect("commit");

    let (status, body) = restore(&f, &f.owner).await;
    assert_eq!(status, 409, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("pair the device again")),
        "{body}"
    );
}

#[tokio::test]
async fn a_member_cannot_undo_somebody_elses_removal() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    assert_eq!(remove(&f, &f.owner).await, 204);

    let (status, body) = restore(&f, &f.member).await;
    assert_eq!(status, 403, "{body}");
}
