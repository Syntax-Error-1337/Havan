//! What a device tells us about an update, and what we refuse to believe.
//!
//! Runs only when `TEST_DATABASE_URL` is set.

use std::sync::Arc;
use std::time::Duration;

use domain::ota::OtaRepo;
use protocol::envelope::OtaReport;
use rumqttc::{AsyncClient, MqttOptions};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;
use worker::automation::Runner;
use worker::ingest::Ingest;

fn ids() -> Uuid {
    Uuid::now_v7()
}

struct Fixture {
    pg: sqlx::PgPool,
    vendor_id: Uuid,
    campaign_id: Uuid,
    device_id: Uuid,
    ingest: Ingest,
}

async fn seed(pg: &sqlx::PgPool) -> (Uuid, Uuid, Uuid) {
    let vendor_id = ids();
    let product_id = ids();
    let user_id = ids();
    let home_id = ids();
    let device_id = ids();
    let firmware_id = ids();
    let campaign_id = ids();

    let simple = vendor_id.simple().to_string();
    let short = &simple[simple.len() - 8..];

    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'OTA Vendor')")
        .bind(vendor_id)
        .bind(format!("o-{short}"))
        .execute(pg)
        .await
        .expect("vendor");

    sqlx::query(
        r#"INSERT INTO products (id, vendor_id, product_key, name, category, thing_model)
           VALUES ($1, $2, $3, 'OTA Bulb', 'light', '{}'::jsonb)"#,
    )
    .bind(product_id)
    .bind(vendor_id)
    .bind(format!("ota_{short}"))
    .execute(pg)
    .await
    .expect("product");

    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("{user_id}@haven.test"))
        .execute(pg)
        .await
        .expect("user");

    sqlx::query("INSERT INTO homes (id, owner_id, name) VALUES ($1, $2, 'OTA flat')")
        .bind(home_id)
        .bind(user_id)
        .execute(pg)
        .await
        .expect("home");

    sqlx::query(
        r#"INSERT INTO devices
             (id, vendor_id, product_id, product_key, home_id, name, serial, secret_hash, state, category)
           VALUES ($1, $2, $3, $4, $5, 'OTA Bulb', $6, 'x', 'claimed', 'light')"#,
    )
    .bind(device_id)
    .bind(vendor_id)
    .bind(product_id)
    .bind(format!("ota_{short}"))
    .bind(home_id)
    .bind(format!("OTA-{}", device_id.simple().to_string()[..8].to_uppercase()))
    .execute(pg)
    .await
    .expect("device");

    sqlx::query(
        r#"INSERT INTO firmware (id, product_id, version, size, sha256, ed25519_sig, object_key, status)
           VALUES ($1, $2, '9.9.9', 1024, $3, 'sig', 'fw/x.bin', 'signed')"#,
    )
    .bind(firmware_id)
    .bind(product_id)
    .bind("0".repeat(64))
    .execute(pg)
    .await
    .expect("firmware");

    sqlx::query(
        r#"INSERT INTO ota_campaigns (id, vendor_id, product_id, firmware_id, rollout_pct, status, started_at)
           VALUES ($1, $2, $3, $4, 100, 'running', now())"#,
    )
    .bind(campaign_id)
    .bind(vendor_id)
    .bind(product_id)
    .bind(firmware_id)
    .execute(pg)
    .await
    .expect("campaign");

    sqlx::query("INSERT INTO ota_jobs (campaign_id, device_id, state) VALUES ($1, $2, 'sent')")
        .bind(campaign_id)
        .bind(device_id)
        .execute(pg)
        .await
        .expect("job");

    (vendor_id, campaign_id, device_id)
}

async fn boot() -> Option<Fixture> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let owner_db = std::env::var("MIGRATE_DATABASE_URL").unwrap_or_else(|_| db.clone());

    let pg = PgPoolOptions::new()
        .max_connections(4)
        .connect(&owner_db)
        .await
        .ok()?;

    let (vendor_id, campaign_id, device_id) = seed(&pg).await;

    let infra_pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(
        &std::env::var("TEST_REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6380".into()),
    )
    .await
    .ok()?;

    let mut opts = MqttOptions::new("ota-progress-test", "127.0.0.1", 1883);
    opts.set_keep_alive(Duration::from_secs(5));
    let (client, _eventloop) = AsyncClient::new(opts, 8);

    let runner = Arc::new(Runner::new(
        infra::PgAutomation::new(infra_pg.clone()),
        client,
        "ota-progress-test".into(),
    ));

    let ingest = Ingest::new(
        infra::PgIngest::new(infra_pg.clone()),
        infra::PgDevices::new(infra_pg.clone()),
        cache,
        runner,
        infra::PgOta::new(infra_pg),
    );

    Some(Fixture {
        pg,
        vendor_id,
        campaign_id,
        device_id,
        ingest,
    })
}

fn report(campaign_id: Uuid, state: &str) -> OtaReport {
    OtaReport {
        v: 1,
        ts: kernel::now_s(),
        campaign_id,
        state: state.to_string(),
        pct: None,
        code: None,
    }
}

async fn job_state(f: &Fixture) -> String {
    sqlx::query_scalar("SELECT state FROM ota_jobs WHERE campaign_id = $1 AND device_id = $2")
        .bind(f.campaign_id)
        .bind(f.device_id)
        .fetch_one(&f.pg)
        .await
        .expect("job")
}

#[tokio::test]
async fn a_device_walks_a_job_from_sent_to_success() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    for state in ["downloading", "verified", "rebooting", "success"] {
        f.ingest
            .handle_ota(f.device_id, &report(f.campaign_id, state))
            .await;
        assert_eq!(job_state(&f).await, state);
    }
}

#[tokio::test]
async fn an_unknown_state_is_refused_rather_than_guessed() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // A firmware typo that were accepted would mark a fleet `success` while the
    // devices were still downloading.
    f.ingest
        .handle_ota(f.device_id, &report(f.campaign_id, "totally-fine"))
        .await;

    assert_eq!(job_state(&f).await, "sent");
}

#[tokio::test]
async fn a_device_cannot_put_its_job_back_in_the_queue() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // `pending` and `sent` are the cloud's to set; a device reporting them is
    // replaying an old message, and honouring it would requeue live work.
    for state in ["pending", "sent"] {
        f.ingest
            .handle_ota(f.device_id, &report(f.campaign_id, state))
            .await;
    }

    assert_eq!(job_state(&f).await, "sent");
}

#[tokio::test]
async fn progress_counts_what_actually_happened() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    f.ingest
        .handle_ota(f.device_id, &report(f.campaign_id, "success"))
        .await;

    let repo = infra::PgOta::new(
        infra::Pg::connect(&std::env::var("TEST_DATABASE_URL").expect("url"), 2)
            .await
            .expect("pg"),
    );

    let progress = repo
        .progress(f.vendor_id, f.campaign_id)
        .await
        .expect("progress");

    assert_eq!(progress.total, 1);
    assert_eq!(progress.success, 1);
    assert!(progress.is_complete());
}

#[tokio::test]
async fn a_finished_device_cannot_be_dragged_back_into_downloading() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    f.ingest
        .handle_ota(f.device_id, &report(f.campaign_id, "success"))
        .await;
    // A replayed report from a device that already finished. Honouring it would
    // make a completed campaign look like it regressed.
    f.ingest
        .handle_ota(f.device_id, &report(f.campaign_id, "downloading"))
        .await;

    assert_eq!(job_state(&f).await, "success");
}
