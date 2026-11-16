//! An unsigned image must not start a rollout.
//!
//! The device verifies a detached Ed25519 signature before writing a byte to flash — that is the
//! `ota_verify` port in the SDK. So a campaign on an unsigned firmware is not a risky rollout, it is
//! a campaign that silently does nothing on every board while the portal reports progress. This test
//! holds the server side of that promise.

use domain::ota::{Campaign, CampaignStatus, OtaRepo};
use kernel::ErrorCode;
use uuid::Uuid;

struct Fixture {
    db_url: String,
    pg: infra::Pg,
    vendor_id: Uuid,
    product_id: Uuid,
    firmware_id: Uuid,
}

async fn fixture(status: &str) -> Option<Fixture> {
    let db_url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pg = infra::Pg::connect(&db_url, 2).await.ok()?;

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&db_url)
        .await
        .ok()?;
    sqlx::query("SET ROLE haven_system")
        .execute(&pool)
        .await
        .ok()?;

    let vendor_id = Uuid::now_v7();
    let product_id = Uuid::now_v7();
    let firmware_id = Uuid::now_v7();
    let tag = &product_id.to_string()[20..32];

    sqlx::query("INSERT INTO vendors (id, name, slug) VALUES ($1, 'OTA Vendor', $2)")
        .bind(vendor_id)
        .bind(format!("ota-{tag}"))
        .execute(&pool)
        .await
        .ok()?;

    let key = format!("hv_ota_{tag}");
    let model = serde_json::json!({
        "product_key": key, "name": "OTA Bulb", "category": "light",
        "capabilities": [{ "name": "switch", "points": { "power": { "dp": "1", "kind": "bool" } } }],
        "constraints": { "allow_unknown_serial": false }
    });
    sqlx::query(
        r#"INSERT INTO products (id, vendor_id, product_key, name, category, thing_model, status, certification)
           VALUES ($1, $2, $3, 'OTA Bulb', 'light', $4, 'published', 'certified')"#,
    )
    .bind(product_id)
    .bind(vendor_id)
    .bind(&key)
    .bind(&model)
    .execute(&pool)
    .await
    .ok()?;

    // `ed25519_sig` is NOT NULL in the schema, so an unsigned image is represented by an empty
    // string with status 'uploaded' — which is what the upload endpoint leaves behind.
    sqlx::query(
        r#"INSERT INTO firmware (id, product_id, version, size, sha256, ed25519_sig, object_key, status)
           VALUES ($1, $2, '1.0.1', 512, $3, '', $4, $5)"#,
    )
    .bind(firmware_id)
    .bind(product_id)
    .bind("a".repeat(64))
    .bind(format!("firmware/{firmware_id}.bin"))
    .bind(status)
    .execute(&pool)
    .await
    .ok()?;

    Some(Fixture {
        db_url,
        pg,
        vendor_id,
        product_id,
        firmware_id,
    })
}

fn campaign(f: &Fixture) -> Campaign {
    Campaign {
        id: Uuid::now_v7(),
        vendor_id: f.vendor_id,
        product_id: f.product_id,
        firmware_id: f.firmware_id,
        rollout_pct: 100,
        status: CampaignStatus::Draft,
        created_at: kernel::now(),
        started_at: None,
    }
}

#[tokio::test]
async fn an_unsigned_firmware_cannot_start_a_campaign() {
    let Some(f) = fixture("uploaded").await else {
        return;
    };
    let ota = infra::PgOta::new(f.pg.clone());

    let err = ota
        .create_campaign(f.vendor_id, &campaign(&f))
        .await
        .unwrap_err();

    assert_eq!(err.code_kind(), ErrorCode::Conflict);
    assert!(
        err.to_string().contains("not signed"),
        "the refusal must say why: {err}"
    );
}

#[tokio::test]
async fn a_revoked_firmware_cannot_start_a_campaign_either() {
    let Some(f) = fixture("revoked").await else {
        return;
    };
    let ota = infra::PgOta::new(f.pg.clone());

    let err = ota
        .create_campaign(f.vendor_id, &campaign(&f))
        .await
        .unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::Conflict);
}

#[tokio::test]
async fn a_signed_firmware_can_start_a_campaign() {
    let Some(f) = fixture("signed").await else {
        return;
    };

    // Give the signature a value, the way the portal's sign step does.
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&f.db_url)
        .await
        .expect("pool");
    sqlx::query("SET ROLE haven_system")
        .execute(&pool)
        .await
        .expect("role");
    sqlx::query("UPDATE firmware SET ed25519_sig = $2 WHERE id = $1")
        .bind(f.firmware_id)
        .bind("b".repeat(128))
        .execute(&pool)
        .await
        .expect("sign");

    let ota = infra::PgOta::new(f.pg.clone());
    ota.create_campaign(f.vendor_id, &campaign(&f))
        .await
        .expect("a signed image is the whole point");
}
