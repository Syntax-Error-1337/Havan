//! Device certificate issuance: the door a device knocks on before it can talk to the broker.
//!
//! `POST /v1/provision/certificate` was reached by nothing in this repository — not a Rust test, not a drill, not a
//! client (`scripts/route-coverage.py --check` found it). That is the wrong endpoint to leave unwatched: it turns a
//! factory secret into mutual-TLS credentials, so every refusal it makes is a security boundary.
//!
//! The four properties below are the ones that would be catastrophic to get wrong, and each is checked by calling the
//! route rather than by reading the handler:
//!   * a wrong secret is refused, and says so without revealing whether the serial exists;
//!   * an unknown product or serial is a 404 rather than a certificate;
//!   * a device that is not in a home yet is refused — an unclaimed device has no home to speak for;
//!   * an oversized payload is refused before the certificate authority or the CSR parser sees it.

use api::{AppState, router};
use domain::identity::hash_password;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

struct Fixture {
    base: String,
    admin: sqlx::PgPool,
    vendor_id: Uuid,
    product_key: String,
    serial: String,
    secret: String,
    device_id: Uuid,
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
        &format!("haven-api-cert-test-{}", Uuid::now_v7()),
    )
    .expect("broker");
    let state = AppState::new(cfg, pg, cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let addr = listener.local_addr().expect("addr");
    let app = router(state);

    // Connect info matters: the issuance limiter is keyed per source, and without a peer every caller in the world
    // shares one bucket.
    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
    });

    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db)
        .await
        .expect("admin pool");

    let vendor_id = Uuid::now_v7();
    let product_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();
    let tail = &vendor_id.simple().to_string()[24..];
    let product_key = format!("cert_{tail}");
    let serial = format!(
        "CERT-{}",
        device_id.simple().to_string()[24..].to_uppercase()
    );
    let secret = "device-Secret-1";

    // One connection, with the row scope set on it: `set_config` is session state, and a pool would run the writes
    // wherever it liked.
    let mut tx = admin.begin().await.expect("seed transaction");
    sqlx::query("SELECT set_config('haven.vendor_id', $1, false)")
        .bind(vendor_id.to_string())
        .execute(&mut *tx)
        .await
        .expect("scope");
    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Cert Vendor')")
        .bind(vendor_id)
        .bind(format!("cert-{tail}"))
        .execute(&mut *tx)
        .await
        .expect("vendor");
    // Published, because `product_by_key` only answers for published products: a draft is not something a device can
    // arrive holding credentials for.
    // A real thing model, not `{}`. `product_by_key` deserialises the stored model and maps a failure to an internal
    // error — which is the right call in production (a published model was validated on publish, so a stored model that
    // will not parse is a bug) and a trap in a fixture: an empty object turns every provisioning call into a 500.
    let model = serde_json::json!({
        "name": "Cert Bulb",
        "category": "light",
        "product_key": product_key,
        "constraints": { "allow_unknown_serial": false },
        "capabilities": [
            { "name": "switch", "points": { "power": { "dp": "1", "kind": "bool" } } }
        ],
    });
    sqlx::query(
        "INSERT INTO products (id, vendor_id, product_key, name, category, thing_model, status)
         VALUES ($1, $2, $3, 'Cert Bulb', 'light', $4, 'published')",
    )
    .bind(product_id)
    .bind(vendor_id)
    .bind(&product_key)
    .bind(&model)
    .execute(&mut *tx)
    .await
    .expect("product");
    // Claimed *and* in a home: the route refuses a device that has no home to speak for it, and that refusal comes
    // before the payload checks — which is why the fixture needs the owner and the home a real claim would have made.
    let owner = Uuid::now_v7();
    let home = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'Cert Owner')")
        .bind(owner)
        .bind(format!("cert-{tail}@haven.dev"))
        .execute(&mut *tx)
        .await
        .expect("owner");
    sqlx::query("INSERT INTO homes (id, name, owner_id) VALUES ($1, 'Cert flat', $2)")
        .bind(home)
        .bind(owner)
        .execute(&mut *tx)
        .await
        .expect("home");

    sqlx::query(
        "INSERT INTO devices (id, vendor_id, product_id, product_key, home_id, name, serial, secret_hash,
                              state, category)
         VALUES ($1, $2, $3, $4, $5, 'Cert Bulb', $6, $7, 'claimed', 'light')",
    )
    .bind(device_id)
    .bind(vendor_id)
    .bind(product_id)
    .bind(&product_key)
    .bind(home)
    .bind(&serial)
    .bind(hash_password(secret).expect("hash"))
    .execute(&mut *tx)
    .await
    .expect("device");
    tx.commit().await.expect("commit");

    Some(Fixture {
        base: format!("http://{addr}"),
        admin,
        vendor_id,
        product_key,
        serial,
        secret: secret.to_string(),
        device_id,
    })
}

async fn post(f: &Fixture, body: serde_json::Value) -> (u16, serde_json::Value) {
    let response = reqwest::Client::new()
        .post(format!("{}/v1/provision/certificate", f.base))
        .json(&body)
        .send()
        .await
        .expect("request");
    let status = response.status().as_u16();
    let text = response.text().await.unwrap_or_default();
    let parsed = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
    (status, parsed)
}

#[tokio::test]
async fn a_wrong_device_secret_gets_a_certificate_never() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let (status, body) = post(
        &f,
        serde_json::json!({ "pk": f.product_key, "sn": f.serial, "secret": "not-the-secret" }),
    )
    .await;

    assert_eq!(status, 403, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("secret")),
        "the refusal names the secret, not the serial: {body}"
    );
}

#[tokio::test]
async fn an_unknown_serial_is_a_404_rather_than_a_certificate() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let (status, body) = post(
        &f,
        serde_json::json!({ "pk": f.product_key, "sn": "CERT-DOES-NOT-EXIST", "secret": f.secret }),
    )
    .await;
    assert_eq!(status, 404, "{body}");

    let (status, body) = post(
        &f,
        serde_json::json!({ "pk": "no_such_product", "sn": f.serial, "secret": f.secret }),
    )
    .await;
    assert_eq!(status, 404, "{body}");
}

#[tokio::test]
async fn a_device_that_is_not_in_a_home_cannot_be_issued_a_certificate() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // The right secret, the right serial, and no home: provisioning is what a claimed device does, and an unclaimed one
    // has nobody to speak for it.
    // Scoped, or the update touches nothing and the test proves nothing: row-level security filters the row out and
    // `execute` reports success for zero rows. The device then stays claimed and the route — correctly — answers 201.
    let mut tx = f.admin.begin().await.expect("transaction");
    sqlx::query("SELECT set_config('haven.vendor_id', $1, false)")
        .bind(f.vendor_id.to_string())
        .execute(&mut *tx)
        .await
        .expect("scope");
    let updated = sqlx::query("UPDATE devices SET state = 'unclaimed' WHERE id = $1")
        .bind(f.device_id)
        .execute(&mut *tx)
        .await
        .expect("unclaim")
        .rows_affected();
    tx.commit().await.expect("commit");
    assert_eq!(updated, 1, "the fixture must actually unclaim the device");

    let (status, body) = post(
        &f,
        serde_json::json!({ "pk": f.product_key, "sn": f.serial, "secret": f.secret }),
    )
    .await;

    assert_eq!(status, 403, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("claimed")),
        "{body}"
    );
}

#[tokio::test]
async fn an_oversized_payload_is_refused_before_the_parser_sees_it() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // The secret is right, so this refusal is about the payload: 9 KiB of "CSR" must not reach the certificate
    // authority, because parsing and signing it is the expensive part.
    let (status, body) = post(
        &f,
        serde_json::json!({
            "pk": f.product_key,
            "sn": f.serial,
            "secret": f.secret,
            "csr_pem": "x".repeat(9 * 1024),
        }),
    )
    .await;

    assert_eq!(status, 422, "{body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("8 KiB")),
        "{body}"
    );
}
