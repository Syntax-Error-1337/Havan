//! Releasing a device: the endpoint that could not succeed.
//!
//! `DELETE /v1/devices/{id}` was documented, the app could reach it, and every call failed with
//! `42501 new row violates row-level security policy` — turned into a bland `503 unavailable`. The
//! policy on `devices` used one USING expression, which Postgres also applies to the new row, and the
//! new row of a release has no `home_id` to be a member of.
//!
//! Two rules are worth pinning here, because both were wrong in different layers:
//!   * an admin of the home can release a device, and the row survives with no home (the licence and the
//!     history hang off it, so deleting it would erase a receipt);
//!   * a member cannot, and is told so by the service rather than by the database.

use api::{AppState, router};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

struct Fixture {
    base: String,
    admin: sqlx::PgPool,
    owner_token: String,
    home_id: Uuid,
    device_id: Uuid,
}

async fn register(base: &str, email: &str) -> Option<String> {
    let tokens: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({ "email": email, "password": "release-Password-1" }))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    tokens["access"].as_str().map(str::to_string)
}

async fn boot() -> Option<Fixture> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;
    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        "haven-api-release-test",
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

    let owner_token = register(&base, &format!("release-{}@haven.dev", Uuid::now_v7())).await?;

    let home: serde_json::Value = client
        .post(format!("{base}/v1/homes"))
        .bearer_auth(&owner_token)
        .json(&serde_json::json!({ "name": "Release flat", "timezone": "UTC" }))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let home_id: Uuid = home["id"].as_str()?.parse().ok()?;

    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db)
        .await
        .ok()?;

    let vendor_id = Uuid::now_v7();
    let product_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();
    let key = format!("rel_{}", &product_id.simple().to_string()[24..]);

    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Release Vendor')")
        .bind(vendor_id)
        .bind(format!("rel-{}", &vendor_id.simple().to_string()[24..]))
        .execute(&admin)
        .await
        .ok()?;
    sqlx::query(
        "INSERT INTO products (id, vendor_id, product_key, name, category, thing_model)
         VALUES ($1, $2, $3, 'Release Bulb', 'light', '{}'::jsonb)",
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
         VALUES ($1, $2, $3, $4, $5, 'Release Bulb', $6, 'x', 'claimed', 'light', false)",
    )
    .bind(device_id)
    .bind(vendor_id)
    .bind(product_id)
    .bind(&key)
    .bind(home_id)
    .bind(format!("REL-{}", &device_id.simple().to_string()[24..]))
    .execute(&admin)
    .await
    .ok()?;

    Some(Fixture {
        base,
        admin,
        owner_token,
        home_id,
        device_id,
    })
}

#[tokio::test]
async fn an_admin_can_release_a_device_and_the_row_survives_without_a_home() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let status = reqwest::Client::new()
        .delete(format!("{}/v1/devices/{}", f.base, f.device_id))
        .bearer_auth(&f.owner_token)
        .send()
        .await
        .expect("delete")
        .status();
    assert_eq!(status.as_u16(), 204, "an owner releasing their own device");

    // The device leaves the home but the row stays: the licence activation and the history receipts
    // hang off it, and a release is not a receipt-burning event.
    let (home, state): (Option<Uuid>, String) =
        sqlx::query_as("SELECT home_id, state FROM devices WHERE id = $1")
            .bind(f.device_id)
            .fetch_one(&f.admin)
            .await
            .expect("row still exists");
    assert!(home.is_none(), "released, not moved elsewhere");
    assert_eq!(state, "released");

    // And it is no longer visible to the customer.
    let after = reqwest::Client::new()
        .get(format!("{}/v1/devices/{}", f.base, f.device_id))
        .bearer_auth(&f.owner_token)
        .send()
        .await
        .expect("get")
        .status();
    assert_eq!(after.as_u16(), 404);
}

#[tokio::test]
async fn a_member_is_refused_by_the_service_not_by_the_database() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // A second account, joined as a member of the same home.
    let member_token = register(&f.base, &format!("member-{}@haven.dev", Uuid::now_v7()))
        .await
        .expect("register member");
    let me: serde_json::Value = reqwest::Client::new()
        .get(format!("{}/v1/me", f.base))
        .bearer_auth(&member_token)
        .send()
        .await
        .expect("me")
        .json()
        .await
        .expect("me json");
    let member_id: Uuid = me["id"].as_str().expect("id").parse().expect("uuid");

    sqlx::query("INSERT INTO home_members (home_id, user_id, role) VALUES ($1, $2, 'member')")
        .bind(f.home_id)
        .bind(member_id)
        .execute(&f.admin)
        .await
        .expect("join");

    let response = reqwest::Client::new()
        .delete(format!("{}/v1/devices/{}", f.base, f.device_id))
        .bearer_auth(&member_token)
        .send()
        .await
        .expect("delete");
    assert_eq!(response.status().as_u16(), 403);

    let body: serde_json::Value = response.json().await.expect("json");
    assert_eq!(body["error"]["code"], "forbidden");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("admin"),
        "the refusal should say who can: {body}"
    );

    // Nothing happened to the row.
    let home: Option<Uuid> = sqlx::query_scalar("SELECT home_id FROM devices WHERE id = $1")
        .bind(f.device_id)
        .fetch_one(&f.admin)
        .await
        .expect("row");
    assert_eq!(home, Some(f.home_id));
}
