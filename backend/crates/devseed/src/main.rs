//! `haven-dev-seed` — a vendor, a portal user and three reference products, on
//! a database that has had migrations applied. Development only.
//!
//! The point is that no demo or test needs a hand-written INSERT: a fresh
//! checkout can be brought to "a vendor can log in and see products" with one
//! command.

use anyhow::Context;
use kernel::TenantCtx;
use uuid::Uuid;

const VENDOR_SLUG: &str = "acme";
const VENDOR_NAME: &str = "Acme Devices";
const PORTAL_EMAIL: &str = "admin@acme.test";
const PORTAL_PASSWORD: &str = "acme-demo-password";
/// Fixed, not random: the portal's login form pre-fills this vendor and the
/// README quotes it. A seed whose identity changes per run cannot be documented.
const VENDOR_ID: Uuid = Uuid::from_u128(0x11111111_1111_7111_8111_111111111111);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = kernel::Config::from_env().context("configuration")?;
    kernel::telemetry::init(&cfg.env);

    let pg = infra::Pg::connect(&cfg.database_url, 2)
        .await
        .context("connect")?;
    let mut tx = pg.scoped(&TenantCtx::system("dev-seed")).await?;

    let vendor_id: Uuid = sqlx::query_scalar(
        r#"INSERT INTO vendors (id, slug, name) VALUES ($1, $2, $3)
           ON CONFLICT (slug) DO UPDATE SET name = EXCLUDED.name
           RETURNING id"#,
    )
    .bind(VENDOR_ID)
    .bind(VENDOR_SLUG)
    .bind(VENDOR_NAME)
    .fetch_one(&mut *tx)
    .await
    .context("vendor")?;

    let hash = domain::identity::hash_password(PORTAL_PASSWORD)?;
    sqlx::query(
        r#"INSERT INTO vendor_users (id, vendor_id, email, display_name, role, password_hash)
           VALUES ($1, $2, $3, 'Acme Admin', 'owner', $4)
           ON CONFLICT (vendor_id, email) DO UPDATE SET password_hash = EXCLUDED.password_hash"#,
    )
    .bind(kernel::new_id())
    .bind(vendor_id)
    .bind(PORTAL_EMAIL)
    .bind(&hash)
    .execute(&mut *tx)
    .await
    .context("vendor user")?;

    for model in reference_models() {
        let id = kernel::new_id();
        sqlx::query(
            r#"INSERT INTO products (id, vendor_id, product_key, name, category, status,
                                     schema_version, thing_model, published_at)
               VALUES ($1,$2,$3,$4,$5,'published',1,$6, now())
               ON CONFLICT (product_key) DO NOTHING"#,
        )
        .bind(id)
        .bind(vendor_id)
        .bind(model["product_key"].as_str().unwrap_or_default())
        .bind(model["name"].as_str().unwrap_or_default())
        .bind(model["category"].as_str().unwrap_or_default())
        .bind(&model)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("product {}", model["product_key"]))?;
    }

    tx.commit().await.context("commit")?;

    println!("vendor_id      {vendor_id}");
    println!("portal login   {PORTAL_EMAIL} / {PORTAL_PASSWORD}");
    println!("products       {}", reference_models().len());
    Ok(())
}

/// The three SKUs the docs, the simulator and the app tests all assume exist.
fn reference_models() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "schema_version": 1,
            "product_key": "hv_acme_bulb1",
            "name": "Acme Bulb E27",
            "category": "light",
            "capabilities": [
                { "name": "switch", "points": { "power": { "dp": "1", "kind": "bool" } } },
                { "name": "brightness", "points": { "level": { "dp": "2", "kind": "int" } } },
                { "name": "color_temp", "points": { "kelvin": { "dp": "3", "kind": "int" } } }
            ],
            "constraints": { "command_timeout_ms": 10000, "allow_unknown_serial": true }
        }),
        serde_json::json!({
            "schema_version": 1,
            "product_key": "hv_acme_plug1",
            "name": "Acme Smart Plug",
            "category": "plug",
            "capabilities": [
                { "name": "switch", "points": { "power": { "dp": "1", "kind": "bool" } } },
                { "name": "energy.power", "points": { "watts": { "dp": "2", "kind": "float" } } }
            ],
            "constraints": { "allow_unknown_serial": true }
        }),
        serde_json::json!({
            "schema_version": 1,
            "product_key": "hv_acme_sensor1",
            "name": "Acme Climate Sensor",
            "category": "sensor",
            "capabilities": [
                { "name": "temperature", "points": { "value": { "dp": "1", "kind": "float" } } },
                { "name": "humidity", "points": { "value": { "dp": "2", "kind": "float" } } },
                { "name": "battery", "points": { "level": { "dp": "3", "kind": "int" } } }
            ],
            "constraints": { "allow_unknown_serial": true }
        }),
    ]
}
