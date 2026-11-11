//! `command.settled`, end to end on the worker side.
//!
//! A command is not done when the API returns 202 — it is done when the device
//! acks and the worker fans that out. Nothing here is a mock: a real Postgres
//! row, a real Redis channel, the real ingest lane.
//!
//! Runs only when `TEST_DATABASE_URL` and `TEST_REDIS_URL` are set, matching the
//! other integration tests in this workspace.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use protocol::envelope::AckMsg;
use rumqttc::{AsyncClient, MqttOptions};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;
use worker::automation::Runner;
use worker::ingest::Ingest;

struct Fixture {
    pg: sqlx::PgPool,
    /// A plain client, kept for pub/sub: a ConnectionManager cannot subscribe.
    subscriber: redis::Client,
    device_id: Uuid,
    home_id: Uuid,
    command_id: Uuid,
    ingest: Ingest,
}

fn ids() -> Uuid {
    Uuid::now_v7()
}

async fn seed(pg: &sqlx::PgPool) -> (Uuid, Uuid, Uuid) {
    let vendor_id = ids();
    let product_id = ids();
    let user_id = ids();
    let home_id = ids();
    let device_id = ids();
    let point_id = ids();
    let command_id = ids();

    // The schema's slug rule is `^[a-z0-9][a-z0-9-]{1,31}$`; a full uuid is too
    // long for it, which is how this failed the first time.
    // The tail, not the head: a v7 uuid starts with a timestamp, so the first
    // eight characters collide across tests that run in the same millisecond.
    let simple = vendor_id.simple().to_string();
    let short = &simple[simple.len() - 8..];
    let slug = format!("v-{short}");
    let product_key = format!("test_{short}");
    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Test Vendor')")
        .bind(vendor_id)
        .bind(&slug)
        .execute(pg)
        .await
        .expect("vendor");

    sqlx::query(
        r#"INSERT INTO products (id, vendor_id, product_key, name, category, thing_model)
           VALUES ($1, $2, $3, 'Test Bulb', 'light', '{}'::jsonb)"#,
    )
    .bind(product_id)
    .bind(vendor_id)
    .bind(&product_key)
    .execute(pg)
    .await
    .expect("product");

    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("{user_id}@haven.test"))
        .execute(pg)
        .await
        .expect("user");

    sqlx::query("INSERT INTO homes (id, owner_id, name) VALUES ($1, $2, 'Test flat')")
        .bind(home_id)
        .bind(user_id)
        .execute(pg)
        .await
        .expect("home");

    sqlx::query("INSERT INTO home_members (home_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(home_id)
        .bind(user_id)
        .execute(pg)
        .await
        .expect("member");

    sqlx::query(
        r#"INSERT INTO devices
             (id, vendor_id, product_id, product_key, home_id, name, serial, secret_hash, state,
                 category)
           VALUES ($1, $2, $3, $4, $5, 'Test Bulb', $6, 'x', 'claimed', 'light')"#,
    )
    .bind(device_id)
    .bind(vendor_id)
    .bind(product_id)
    .bind(&product_key)
    .bind(home_id)
    // Serial rule: `^[A-Z0-9-]+$`.
    .bind(format!(
        "TEST-{}",
        device_id.simple().to_string()[..8].to_uppercase()
    ))
    .execute(pg)
    .await
    .expect("device");

    sqlx::query(
        r#"INSERT INTO device_points
             (id, device_id, vendor_id, capability, slot, code, dp, kind, writable)
           VALUES ($1, $2, $3, 'switch', 'power', 'switch.power', '1', 'bool', true)"#,
    )
    .bind(point_id)
    .bind(device_id)
    .bind(vendor_id)
    .execute(pg)
    .await
    .expect("point");

    sqlx::query(
        r#"INSERT INTO commands
             (id, vendor_id, device_id, point_id, op, payload, status, dispatched_at)
           VALUES ($1, $2, $3, $4, 'set', '{"value":true}'::jsonb, 'queued', now())"#,
    )
    .bind(command_id)
    .bind(vendor_id)
    .bind(device_id)
    .bind(point_id)
    .execute(pg)
    .await
    .expect("command");

    (device_id, home_id, command_id)
}

async fn boot() -> Option<Fixture> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis_url = std::env::var("TEST_REDIS_URL").ok()?;

    // Seeding runs as the owner: the app role is subject to RLS and has no
    // vendor context to satisfy the policy with. The lane under test still runs
    // as `db`, so the policy is exercised where it matters.
    let owner_db = std::env::var("MIGRATE_DATABASE_URL").unwrap_or_else(|_| db.clone());
    let pg = PgPoolOptions::new()
        .max_connections(2)
        .connect(&owner_db)
        .await
        .ok()?;
    let cache = infra::Cache::connect(&redis_url).await.ok()?;
    let subscriber = redis::Client::open(redis_url).ok()?;

    let (device_id, home_id, command_id) = seed(&pg).await;

    // The runner needs a client, not a broker: nothing is published in this test.
    let mut opts = MqttOptions::new("worker-test", "127.0.0.1", 1883);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, _eventloop) = AsyncClient::new(opts, 8);

    let infra_pg = infra::Pg::connect(&db, 2).await.ok()?;
    let automation = Arc::new(Runner::new(
        infra::PgAutomation::new(infra_pg.clone()),
        client,
        "worker-test".into(),
    ));

    let ingest = Ingest::new(
        infra::PgIngest::new(infra_pg.clone()),
        infra::PgDevices::new(infra_pg.clone()),
        cache.clone(),
        automation,
        infra::PgOta::new(infra_pg),
    );

    Some(Fixture {
        pg,
        subscriber,
        device_id,
        home_id,
        command_id,
        ingest,
    })
}

fn ack(command_id: Uuid, ok: bool) -> AckMsg {
    AckMsg {
        v: 1,
        id: command_id,
        ts: kernel::now_s(),
        ok,
        dp: Some("1".into()),
        value: Some(protocol::envelope::DpValue::Bool(true)),
        code: None,
        msg: None,
    }
}

#[tokio::test]
async fn an_ack_settles_the_command_and_reaches_the_home_channel() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // Subscribe before the ack: Redis has no replay, and a test that subscribes
    // after the publish proves nothing.
    let channel = format!("home:{}", f.home_id);
    let mut pubsub = f.subscriber.get_async_pubsub().await.expect("pubsub");
    pubsub.subscribe(&channel).await.expect("subscribe");
    let mut stream = pubsub.on_message();

    f.ingest.handle_ack(ack(f.command_id, true)).await;

    let received = tokio::time::timeout(Duration::from_millis(2000), stream.next())
        .await
        .expect("an event within 2s")
        .expect("a message");
    let payload: serde_json::Value =
        serde_json::from_str(&received.get_payload::<String>().expect("utf8")).expect("json");

    assert_eq!(payload["t"], "command.settled");
    assert_eq!(payload["command_id"], f.command_id.to_string());
    assert_eq!(payload["device_id"], f.device_id.to_string());
    assert_eq!(payload["status"], "applied");

    let status: String = sqlx::query_scalar("SELECT status FROM commands WHERE id = $1")
        .bind(f.command_id)
        .fetch_one(&f.pg)
        .await
        .expect("command row");
    assert_eq!(status, "applied");
}

#[tokio::test]
async fn a_redelivered_ack_does_not_publish_twice() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    f.ingest.handle_ack(ack(f.command_id, true)).await;

    // Subscribe after the first settle and expect silence: a redelivery must not
    // resurrect a command, and must not wake every open app for nothing.
    let mut pubsub = f.subscriber.get_async_pubsub().await.expect("pubsub");
    pubsub
        .subscribe(&format!("home:{}", f.home_id))
        .await
        .expect("subscribe");
    let mut stream = pubsub.on_message();

    f.ingest.handle_ack(ack(f.command_id, true)).await;

    let second = tokio::time::timeout(Duration::from_millis(800), stream.next()).await;
    assert!(second.is_err(), "a redelivered ack published again");

    let status: String = sqlx::query_scalar("SELECT status FROM commands WHERE id = $1")
        .bind(f.command_id)
        .fetch_one(&f.pg)
        .await
        .expect("command row");
    assert_eq!(status, "applied");
}

#[tokio::test]
async fn an_ack_for_an_unknown_command_is_ignored() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let mut pubsub = f.subscriber.get_async_pubsub().await.expect("pubsub");
    pubsub
        .subscribe(&format!("home:{}", f.home_id))
        .await
        .expect("subscribe");
    let mut stream = pubsub.on_message();

    f.ingest.handle_ack(ack(Uuid::now_v7(), true)).await;

    let none = tokio::time::timeout(Duration::from_millis(800), stream.next()).await;
    assert!(none.is_err(), "an unknown command published an event");
}

#[tokio::test]
async fn a_rejected_ack_settles_as_rejected_with_its_reason() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    let mut failure = ack(f.command_id, false);
    failure.code = Some("out_of_range".into());

    f.ingest.handle_ack(failure).await;

    let (status, code): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_code FROM commands WHERE id = $1")
            .bind(f.command_id)
            .fetch_one(&f.pg)
            .await
            .expect("command row");

    // The device's own words reach the phone; a generic "failed" would send the
    // user to support for something the firmware already explained.
    assert_eq!(status, "rejected");
    assert_eq!(code.as_deref(), Some("out_of_range"));
}
