//! The OTA queue hands each job to exactly one worker.
//!
//! `claim_jobs` is the one queue that always did this correctly — it flips `state` to `sent` inside
//! the same statement that selects — but nothing asserted it, and the other three queues were all
//! found broken by exactly this question. Two dispatchers firing the same update means a device
//! downloads the same firmware twice and its job's attempt count lies about how many tries it took.

use domain::ota::OtaRepo;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::{Mutex, MutexGuard, OnceCell};
use uuid::Uuid;

/// The queue is global, so these tests take turns for the same reason the others do.
static CLAIM: OnceCell<Mutex<()>> = OnceCell::const_new();

async fn one_claim_at_a_time() -> MutexGuard<'static, ()> {
    CLAIM
        .get_or_init(|| async { Mutex::new(()) })
        .await
        .lock()
        .await
}

struct Seeded {
    pg: sqlx::PgPool,
    repo: infra::PgOta,
    vendor_id: Uuid,
}

async fn boot() -> Option<Seeded> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let owner_db = std::env::var("TEST_MIGRATE_DATABASE_URL").unwrap_or_else(|_| db.clone());
    let pg = PgPoolOptions::new()
        .max_connections(4)
        .connect(&owner_db)
        .await
        .ok()?;
    let infra_pg = infra::Pg::connect(&db, 4).await.ok()?;
    let vendor_id = Uuid::now_v7();

    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Claim Vendor')")
        .bind(vendor_id)
        .bind(format!("cl-{}", &vendor_id.simple().to_string()[24..]))
        .execute(&pg)
        .await
        .ok()?;

    Some(Seeded {
        pg,
        repo: infra::PgOta::new(infra_pg),
        vendor_id,
    })
}

impl Seeded {
    /// A running campaign with `jobs` pending jobs, returning the device ids.
    async fn campaign(&self, jobs: usize) -> Vec<Uuid> {
        let product_id = Uuid::now_v7();
        let firmware_id = Uuid::now_v7();
        let campaign_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let home_id = Uuid::now_v7();
        let short = &product_id.simple().to_string()[24..];

        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(format!("{user_id}@haven.test"))
            .execute(&self.pg)
            .await
            .expect("user");
        sqlx::query("INSERT INTO homes (id, owner_id, name) VALUES ($1, $2, 'Claim flat')")
            .bind(home_id)
            .bind(user_id)
            .execute(&self.pg)
            .await
            .expect("home");
        sqlx::query(
            r#"INSERT INTO products (id, vendor_id, product_key, name, category, thing_model)
               VALUES ($1, $2, $3, 'Claim Bulb', 'light', '{}'::jsonb)"#,
        )
        .bind(product_id)
        .bind(self.vendor_id)
        .bind(format!("claim_{short}"))
        .execute(&self.pg)
        .await
        .expect("product");
        sqlx::query(
            r#"INSERT INTO firmware (id, product_id, version, size, sha256, ed25519_sig, object_key, status)
               VALUES ($1, $2, '1.0.0', 64, $3, 'sig', 'fw/c.bin', 'signed')"#,
        )
        .bind(firmware_id)
        .bind(product_id)
        .bind("0".repeat(64))
        .execute(&self.pg)
        .await
        .expect("firmware");
        sqlx::query(
            r#"INSERT INTO ota_campaigns (id, vendor_id, product_id, firmware_id, rollout_pct, status, started_at)
               VALUES ($1, $2, $3, $4, 100, 'running', now())"#,
        )
        .bind(campaign_id)
        .bind(self.vendor_id)
        .bind(product_id)
        .bind(firmware_id)
        .execute(&self.pg)
        .await
        .expect("campaign");

        let mut devices = Vec::new();
        for _ in 0..jobs {
            let device_id = Uuid::now_v7();
            let tail = device_id.simple().to_string();
            sqlx::query(
                r#"INSERT INTO devices
                     (id, vendor_id, product_id, product_key, home_id, name, serial, secret_hash, state, category)
                   VALUES ($1, $2, $3, $4, $5, 'Claim Bulb', $6, 'x', 'claimed', 'light')"#,
            )
            .bind(device_id)
            .bind(self.vendor_id)
            .bind(product_id)
            .bind(format!("claim_{short}"))
            .bind(home_id)
            .bind(format!("CL-{}", tail[tail.len() - 8..].to_uppercase()))
            .execute(&self.pg)
            .await
            .expect("device");
            // Oldest in the queue on purpose: `claim_jobs` orders by `updated_at`, and the shared
            // test database always holds other tests' pending jobs. Sorting first means a claim of
            // exactly this many rows returns exactly these rows, so the test cannot pass by
            // accident and cannot disturb anybody else's fixtures.
            sqlx::query(
                "INSERT INTO ota_jobs (campaign_id, device_id, state, updated_at)
                 VALUES ($1, $2, 'pending', '2020-01-01')",
            )
            .bind(campaign_id)
            .bind(device_id)
            .execute(&self.pg)
            .await
            .expect("job");
            devices.push(device_id);
        }
        devices
    }
}

#[tokio::test]
async fn two_dispatchers_never_claim_the_same_job() {
    let _only = one_claim_at_a_time().await;
    let Some(s) = boot().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };

    let devices = s.campaign(4).await;

    let (a, b) = tokio::join!(
        s.repo.claim_jobs(devices.len() as i64),
        s.repo.claim_jobs(devices.len() as i64)
    );
    let a = a.expect("first dispatcher");
    let b = b.expect("second dispatcher");

    let mine_a: Vec<Uuid> = a
        .iter()
        .map(|j| j.device_id)
        .filter(|d| devices.contains(d))
        .collect();
    let mine_b: Vec<Uuid> = b
        .iter()
        .map(|j| j.device_id)
        .filter(|d| devices.contains(d))
        .collect();

    assert_eq!(
        mine_a.len() + mine_b.len(),
        devices.len(),
        "every pending job is claimed exactly once across the two dispatchers"
    );
    for device in &mine_a {
        assert!(
            !mine_b.contains(device),
            "a job must not be handed out twice"
        );
    }

    // The state moved with the claim, so a third dispatcher sees nothing to do.
    let c = s
        .repo
        .claim_jobs(devices.len() as i64)
        .await
        .expect("third dispatcher");
    let mine_c: Vec<Uuid> = c
        .iter()
        .map(|j| j.device_id)
        .filter(|d| devices.contains(d))
        .collect();
    assert!(mine_c.is_empty(), "a claimed job is not pending any more");
}
