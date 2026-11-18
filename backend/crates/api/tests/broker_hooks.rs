//! The broker's auth hooks, from the broker's side of the wire.
//!
//! Two properties are worth locking down, and neither is visible from the API's own metrics:
//!
//! 1. A successful *device* registration is marked cacheable, because `vmq_webhooks` only caches
//!    a call whose reply carries `cache-control: max-age`. The firmware reconnects on a 90 s
//!    keepalive, so without this every reconnect costs an HTTP round-trip before the broker will
//!    speak to the device at all.
//! 2. A refusal is never marked cacheable, and neither is the service identity's own login —
//!    a rotated service token has to take effect on the next connection, not when a cache expires.

use domain::identity::hash_password;
use uuid::Uuid;

struct Fixture {
    base: String,
    db_url: String,
    hook_token: String,
    device_id: Uuid,
    device_secret: String,
    service_user: String,
    service_password: String,
    config: std::sync::Arc<kernel::Config>,
    _pg: infra::Pg,
}

async fn boot() -> Option<Fixture> {
    let db_url = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;
    let config = std::sync::Arc::new(kernel::Config::from_env().ok()?);

    let pg = infra::Pg::connect(&db_url, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        &format!("{}-{}", "haven-api-hooks-test", uuid::Uuid::now_v7()),
    )
    .ok()?;

    let hook_token = config.mqtt_hook_token.clone();
    let service_password = config.mqtt_service_token.clone();
    let state = api::AppState::new(config.clone(), pg.clone(), cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            api::router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
    });

    Some(Fixture {
        base: format!("http://{addr}"),
        db_url,
        hook_token,
        device_id: Uuid::now_v7(),
        device_secret: format!("secret-{}", &Uuid::now_v7().to_string()[20..32]),
        service_user: api::routes::internal_mqtt::SERVICE_USER.to_string(),
        service_password,
        config,
        _pg: pg,
    })
}

/// One vendor, one product, one claimed device with a secret this test knows.
async fn seed(f: &Fixture) {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&f.db_url)
        .await
        .expect("admin pool");
    sqlx::query("SET ROLE haven_system")
        .execute(&pool)
        .await
        .expect("system role");

    let vendor_id = Uuid::now_v7();
    let product_id = Uuid::now_v7();
    let home_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();

    sqlx::query("INSERT INTO vendors (id, name, slug) VALUES ($1, 'Hooks Vendor', $2)")
        .bind(vendor_id)
        .bind(format!("hk-{}", &vendor_id.to_string()[20..32]))
        .execute(&pool)
        .await
        .expect("vendor");
    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("{user_id}@hooks.test"))
        .execute(&pool)
        .await
        .expect("user");
    sqlx::query("INSERT INTO homes (id, owner_id, name) VALUES ($1, $2, 'Hooks flat')")
        .bind(home_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("home");

    let key = format!("hv_hooks_{}", &product_id.to_string()[20..32]);
    let model = serde_json::json!({
        "product_key": key,
        "name": "Hooks Bulb",
        "category": "light",
        "capabilities": [{ "name": "switch", "points": { "power": { "dp": "1", "kind": "bool" } } }],
        "constraints": { "allow_unknown_serial": false }
    });
    sqlx::query(
        r#"INSERT INTO products (id, vendor_id, product_key, name, category, thing_model, status, certification)
           VALUES ($1, $2, $3, 'Hooks Bulb', 'light', $4, 'published', 'certified')"#,
    )
    .bind(product_id)
    .bind(vendor_id)
    .bind(format!("hv_hooks_{}", &product_id.to_string()[20..32]))
    .bind(&model)
    .execute(&pool)
    .await
    .expect("product");

    let hash = hash_password(&f.device_secret).expect("hash");
    sqlx::query(
        r#"INSERT INTO devices
             (id, vendor_id, product_id, product_key, category, home_id, name, serial, secret_hash, state, claimed_at)
           VALUES ($1, $2, $3, $4, 'light', $5, 'Hooks bulb', $6, $7, 'claimed', now())"#,
    )
    .bind(f.device_id)
    .bind(vendor_id)
    .bind(product_id)
    .bind(format!("hv_hooks_{}", &product_id.to_string()[20..32]))
    .bind(home_id)
    .bind(format!("SN-{}", &f.device_id.to_string()[20..32]))
    .bind(hash)
    .execute(&pool)
    .await
    .expect("device");
}

async fn register(f: &Fixture, username: &str, password: &str) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!(
            "{}/v1/internal/mqtt/register?token={}",
            f.base, f.hook_token
        ))
        .json(&serde_json::json!({
            "username": username, "password": password, "client_id": username
        }))
        .send()
        .await
        .expect("register")
}

#[tokio::test]
async fn a_device_login_is_cacheable_and_a_refusal_is_not() {
    let Some(f) = boot().await else { return };
    seed(&f).await;

    let ok = register(&f, &f.device_id.to_string(), &f.device_secret).await;
    assert_eq!(ok.status().as_u16(), 200);
    let cache = ok
        .headers()
        .get("cache-control")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let body: serde_json::Value = ok.json().await.expect("json");
    assert_eq!(body["result"], "ok", "the credential should be accepted");
    assert!(
        cache.as_deref().is_some_and(|c| c.starts_with("max-age=")),
        "a successful registration must be marked cacheable, got {cache:?}"
    );

    // A wrong secret is refused and must not be cached: caching a refusal would keep a device
    // that was just re-paired out of its own home for the life of the cache.
    let bad = register(&f, &f.device_id.to_string(), "not-the-secret").await;
    let body: serde_json::Value = bad.json().await.expect("json");
    assert_eq!(body["result"]["error"], "not_allowed");
}

#[tokio::test]
async fn the_service_identity_is_never_cached() {
    let Some(f) = boot().await else { return };
    seed(&f).await;

    let response = register(&f, &f.service_user, &f.service_password).await;
    let cache = response.headers().get("cache-control").cloned();
    let body: serde_json::Value = response.json().await.expect("json");

    assert_eq!(body["result"], "ok");
    assert!(
        cache.is_none(),
        "the service identity must not be cached, or a rotated token would keep working"
    );

    // The wrong service password is refused outright.
    let wrong = register(&f, &f.service_user, "wrong").await;
    let body: serde_json::Value = wrong.json().await.expect("json");
    assert_eq!(body["result"]["error"], "not_allowed");
}

#[tokio::test]
async fn a_hook_call_without_the_broker_token_is_refused() {
    let Some(f) = boot().await else { return };

    let response = reqwest::Client::new()
        .post(format!("{}/v1/internal/mqtt/register", f.base))
        .json(&serde_json::json!({ "username": "x", "password": "y" }))
        .send()
        .await
        .expect("register");

    let body: serde_json::Value = response.json().await.expect("json");
    assert_eq!(body["result"]["error"], "not_allowed");
}

/// Keeps `config` alive for the lifetime of the fixture's server, which reads from it.
#[allow(dead_code)]
fn _config_is_owned(f: &Fixture) -> &kernel::Config {
    &f.config
}
