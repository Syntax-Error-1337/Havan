//! What happens to an offer nobody answers.
//!
//! A job stuck at `sent` and a campaign stuck at `running` are the two ways an
//! OTA dashboard lies: both look like work in progress, forever.
//!
//! Runs only when `TEST_DATABASE_URL` is set.

use std::time::Duration;

use domain::ota::{CampaignStatus, JobStatus, OtaRepo};
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

fn ids() -> Uuid {
    Uuid::now_v7()
}

struct Seeded {
    pg: sqlx::PgPool,
    repo: infra::PgOta,
    vendor_id: Uuid,
}

async fn boot() -> Option<Seeded> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let owner_db = std::env::var("MIGRATE_DATABASE_URL").unwrap_or_else(|_| db.clone());
    let pg = PgPoolOptions::new()
        .max_connections(4)
        .connect(&owner_db)
        .await
        .ok()?;
    let infra_pg = infra::Pg::connect(&db, 4).await.ok()?;
    let vendor_id = ids();

    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Sweep Vendor')")
        .bind(vendor_id)
        .bind(format!("s-{}", &vendor_id.simple().to_string()[24..]))
        .execute(&pg)
        .await
        .ok()?;

    Some(Seeded {
        pg,
        repo: infra::PgOta::new(infra_pg),
        vendor_id,
    })
}

/// A running campaign with one job per entry in `states`.
async fn campaign(s: &Seeded, states: &[&str]) -> (Uuid, Vec<Uuid>) {
    let product_id = ids();
    let firmware_id = ids();
    let campaign_id = ids();
    let user_id = ids();
    let home_id = ids();

    sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
        .bind(user_id)
        .bind(format!("{user_id}@haven.test"))
        .execute(&s.pg)
        .await
        .expect("user");

    sqlx::query("INSERT INTO homes (id, owner_id, name) VALUES ($1, $2, 'Sweep flat')")
        .bind(home_id)
        .bind(user_id)
        .execute(&s.pg)
        .await
        .expect("home");

    let mut tx = s.pg.begin().await.expect("tx");
    let db = &mut *tx;

    let short = &product_id.simple().to_string()[24..];
    sqlx::query(
        r#"INSERT INTO products (id, vendor_id, product_key, name, category, thing_model)
           VALUES ($1, $2, $3, 'Sweep Bulb', 'light', '{}'::jsonb)"#,
    )
    .bind(product_id)
    .bind(s.vendor_id)
    .bind(format!("sweep_{short}"))
    .execute(&mut *db)
    .await
    .expect("product");

    sqlx::query(
        r#"INSERT INTO firmware (id, product_id, version, size, sha256, ed25519_sig, object_key, status)
           VALUES ($1, $2, '1.0.0', 64, $3, 'sig', 'fw/s.bin', 'signed')"#,
    )
    .bind(firmware_id)
    .bind(product_id)
    .bind("0".repeat(64))
    .execute(&mut *db)
    .await
    .expect("firmware");

    sqlx::query(
        r#"INSERT INTO ota_campaigns (id, vendor_id, product_id, firmware_id, rollout_pct, status, started_at)
           VALUES ($1, $2, $3, $4, 100, 'running', now())"#,
    )
    .bind(campaign_id)
    .bind(s.vendor_id)
    .bind(product_id)
    .bind(firmware_id)
    .execute(&mut *db)
    .await
    .expect("campaign");

    let mut devices = Vec::new();
    for state in states {
        let device_id = ids();
        sqlx::query(
            r#"INSERT INTO devices
                 (id, vendor_id, product_id, product_key, home_id, name, serial, secret_hash, state, category)
               VALUES ($1, $2, $3, $4, $5, 'Sweep Bulb', $6, 'x', 'claimed', 'light')"#,
        )
        .bind(device_id)
        .bind(s.vendor_id)
        .bind(product_id)
        .bind(format!("sweep_{short}"))
        .bind(home_id)
        // Tail, not head: a v7 uuid starts with a timestamp, so the first eight
        // characters are identical for devices created in the same millisecond.
        .bind({
            let simple = device_id.simple().to_string();
            format!("SW-{}", simple[simple.len() - 8..].to_uppercase())
        })
        .execute(&mut *db)
        .await
        .expect("device");

        sqlx::query("INSERT INTO ota_jobs (campaign_id, device_id, state) VALUES ($1, $2, $3)")
            .bind(campaign_id)
            .bind(device_id)
            .bind(state)
            .execute(&mut *db)
            .await
            .expect("job");

        devices.push(device_id);
    }

    tx.commit().await.expect("commit");

    (campaign_id, devices)
}

async fn age(pg: &sqlx::PgPool, campaign_id: Uuid, seconds: i64) {
    sqlx::query(
        "UPDATE ota_jobs SET updated_at = now() - make_interval(secs => $2) WHERE campaign_id = $1",
    )
    .bind(campaign_id)
    .bind(seconds as f64)
    .execute(pg)
    .await
    .expect("age");
}

async fn state_of(pg: &sqlx::PgPool, campaign_id: Uuid, device_id: Uuid) -> String {
    sqlx::query_scalar("SELECT state FROM ota_jobs WHERE campaign_id = $1 AND device_id = $2")
        .bind(campaign_id)
        .bind(device_id)
        .fetch_one(pg)
        .await
        .expect("job")
}

#[tokio::test]
async fn a_silent_device_is_retried_while_it_has_attempts_left() {
    let Some(s) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL not set");
        return;
    };
    let (campaign_id, devices) = campaign(&s, &["sent"]).await;
    age(&s.pg, campaign_id, 3600).await;

    s.repo.sweep_stale(60, 3).await.expect("sweep");

    assert_eq!(state_of(&s.pg, campaign_id, devices[0]).await, "pending");
}

#[tokio::test]
async fn a_device_that_never_answers_is_failed_with_a_reason() {
    let Some(s) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL not set");
        return;
    };
    let (campaign_id, devices) = campaign(&s, &["sent"]).await;
    sqlx::query("UPDATE ota_jobs SET attempts = 3 WHERE campaign_id = $1")
        .bind(campaign_id)
        .execute(&s.pg)
        .await
        .expect("attempts");
    age(&s.pg, campaign_id, 3600).await;

    s.repo.sweep_stale(60, 3).await.expect("sweep");

    assert_eq!(state_of(&s.pg, campaign_id, devices[0]).await, "failed");
    let error: Option<String> =
        sqlx::query_scalar("SELECT error FROM ota_jobs WHERE campaign_id = $1")
            .bind(campaign_id)
            .fetch_one(&s.pg)
            .await
            .expect("error column");
    // "failed" with no reason is a support ticket that starts from zero.
    assert!(error.unwrap_or_default().contains("gave up"));
}

#[tokio::test]
async fn a_fresh_job_is_left_alone() {
    let Some(s) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL not set");
        return;
    };
    let (campaign_id, devices) = campaign(&s, &["sent"]).await;

    s.repo.sweep_stale(3600, 3).await.expect("sweep");

    // A device that has been working for ten minutes is not a device that has
    // failed; sweeping it early would restart a download that was going fine.
    assert_eq!(state_of(&s.pg, campaign_id, devices[0]).await, "sent");
}

#[tokio::test]
async fn a_campaign_closes_when_every_job_is_final() {
    let Some(s) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL not set");
        return;
    };
    let (campaign_id, _) = campaign(&s, &["success", "failed", "skipped"]).await;

    s.repo.complete_campaigns().await.expect("complete");

    let campaign = s
        .repo
        .campaign(s.vendor_id, campaign_id)
        .await
        .expect("campaign")
        .expect("exists");
    assert_eq!(campaign.status, CampaignStatus::Completed);
}

#[tokio::test]
async fn a_campaign_with_work_in_flight_stays_running() {
    let Some(s) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL not set");
        return;
    };
    let (campaign_id, _) = campaign(&s, &["success", "downloading"]).await;

    s.repo.complete_campaigns().await.expect("complete");

    let campaign = s
        .repo
        .campaign(s.vendor_id, campaign_id)
        .await
        .expect("campaign");
    assert_eq!(campaign.expect("exists").status, CampaignStatus::Running);
}

#[tokio::test]
async fn a_campaign_that_matched_nobody_is_not_quietly_closed() {
    let Some(s) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL not set");
        return;
    };
    let (campaign_id, _) = campaign(&s, &[]).await;

    s.repo.complete_campaigns().await.expect("complete");

    let campaign = s
        .repo
        .campaign(s.vendor_id, campaign_id)
        .await
        .expect("campaign");
    // Zero jobs means the rollout matched nothing. Closing it would hide a
    // vendor's mistake behind a green tick.
    assert_eq!(campaign.expect("exists").status, CampaignStatus::Running);
}

#[tokio::test]
async fn mark_job_records_the_error_it_was_given() {
    let Some(s) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL not set");
        return;
    };
    let (campaign_id, devices) = campaign(&s, &["sent"]).await;

    s.repo
        .mark_job(campaign_id, devices[0], JobStatus::Failed, None)
        .await
        .expect("mark");

    // Nothing invented: no error was reported, so none is stored.
    let error: Option<String> =
        sqlx::query_scalar("SELECT error FROM ota_jobs WHERE campaign_id = $1")
            .bind(campaign_id)
            .fetch_one(&s.pg)
            .await
            .expect("error column");
    assert_eq!(error, None);

    tokio::time::sleep(Duration::from_millis(10)).await;
}
