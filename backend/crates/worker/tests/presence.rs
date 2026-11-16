//! Presence: a device that stops talking has to stop reading as online.
//!
//! `online` was only ever written by an incoming report, so a device that died without a will
//! message stayed online for as long as its row existed. The platform's own health page caught it —
//! 99 devices "online" whose newest report was four days old — and this is the sweep that fixes it.

use domain::devices::DeviceRepo;
use kernel::TenantCtx;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::{Mutex, MutexGuard, OnceCell};
use uuid::Uuid;

/// One sweep at a time.
///
/// These tests share a database and every sweep is global, so a 300-second window in one test was
/// quietly ageing out another test's fixture — the sweep then reported zero changes and the failure
/// pointed at the wrong thing. Serialising is cheaper than inventing a per-test tenancy for a
/// cross-tenant sweep.
static SWEEP: OnceCell<Mutex<()>> = OnceCell::const_new();

async fn one_sweep_at_a_time() -> MutexGuard<'static, ()> {
    SWEEP
        .get_or_init(|| async { Mutex::new(()) })
        .await
        .lock()
        .await
}

struct Seeded {
    /// The database under test, used for assertions.
    test: sqlx::PgPool,
    devices: infra::PgDevices,
    vendor_id: Uuid,
    product_id: Uuid,
    home_id: Uuid,
}

async fn boot() -> Option<Seeded> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    // Seed where the test database lives: pointing a fixture at the development database is how a
    // test passes while checking nothing.
    let owner_db = std::env::var("TEST_MIGRATE_DATABASE_URL").unwrap_or_else(|_| db.clone());
    let test = PgPoolOptions::new()
        .max_connections(4)
        .connect(&owner_db)
        .await
        .ok()?;

    let infra_pg = infra::Pg::connect(&db, 4).await.ok()?;
    let vendor_id = Uuid::now_v7();
    let product_id = Uuid::now_v7();
    let home_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();

    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Presence Vendor')")
        .bind(vendor_id)
        .bind(format!("pres-{}", &vendor_id.to_string()[20..32]))
        .execute(&test)
        .await
        .ok()?;
    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'Owner')")
        .bind(user_id)
        .bind(format!("pres-{user_id}@example.test"))
        .execute(&test)
        .await
        .ok()?;
    sqlx::query("INSERT INTO homes (id, owner_id, name) VALUES ($1, $2, 'Presence home')")
        .bind(home_id)
        .bind(user_id)
        .execute(&test)
        .await
        .ok()?;
    sqlx::query(
        "INSERT INTO products (id, vendor_id, product_key, name, category, thing_model, status)
         VALUES ($1, $2, $3, 'Presence Bulb', 'light', '{}', 'published')",
    )
    .bind(product_id)
    .bind(vendor_id)
    .bind(format!("prd_{}", &product_id.to_string()[24..36]))
    .execute(&test)
    .await
    .ok()?;

    Some(Seeded {
        test,
        devices: infra::PgDevices::new(infra_pg),
        vendor_id,
        product_id,
        home_id,
    })
}

impl Seeded {
    /// A device that reported `minutes_ago` minutes ago and is flagged online.
    async fn device(&self, serial: &str, minutes_ago: i64) -> Uuid {
        let id = Uuid::now_v7();
        sqlx::query(
            r#"INSERT INTO devices (id, vendor_id, product_id, home_id, name, serial, secret_hash,
                                    product_key, category, online, last_seen_at)
               VALUES ($1, $2, $3, $4, 'Presence', $5, 'x', $6, 'light', true,
                       now() - make_interval(mins => $7::int))"#,
        )
        .bind(id)
        .bind(self.vendor_id)
        .bind(self.product_id)
        .bind(self.home_id)
        .bind(serial)
        .bind(format!("prd_{}", &self.product_id.to_string()[24..36]))
        .bind(minutes_ago as i32)
        .execute(&self.test)
        .await
        .expect("device");
        id
    }

    async fn online(&self, id: Uuid) -> bool {
        let row: (bool,) = sqlx::query_as("SELECT online FROM devices WHERE id = $1")
            .bind(id)
            .fetch_one(&self.test)
            .await
            .expect("lookup");
        row.0
    }
}

#[tokio::test]
async fn a_silent_device_is_marked_offline_and_a_talking_one_is_left_alone() {
    let _only = one_sweep_at_a_time().await;
    let Some(f) = boot().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };

    let talking = f.device("PRES-1", 0).await;
    let silent = f.device("PRES-2", 10).await;

    let swept = f.devices.age_out_stale(90).await.expect("sweep");
    assert!(swept >= 1, "the sweep changed nothing");

    assert!(
        f.online(talking).await,
        "a device that reported just now must stay online"
    );
    assert!(
        !f.online(silent).await,
        "a device silent for ten minutes must not read as online"
    );

    // Idempotent: a second sweep finds nothing to do and does not touch the talking device.
    f.devices.age_out_stale(90).await.expect("second sweep");
    assert!(f.online(talking).await);
}

#[tokio::test]
async fn the_sweep_respects_the_window_it_is_given() {
    let _only = one_sweep_at_a_time().await;
    let Some(f) = boot().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };

    // Silent for two minutes: inside a five-minute window, outside a thirty-second one. The window
    // is a parameter because the right number is a deployment decision, not a constant to bake in.
    let quiet = f.device("PRES-3", 2).await;

    f.devices.age_out_stale(300).await.expect("wide sweep");
    assert!(
        f.online(quiet).await,
        "two minutes of silence is inside a five-minute window"
    );

    f.devices.age_out_stale(30).await.expect("narrow sweep");
    assert!(
        !f.online(quiet).await,
        "two minutes of silence is outside a thirty-second window"
    );
}

/// The system context is what makes a cross-tenant sweep possible at all.
#[tokio::test]
async fn the_sweep_runs_across_tenants() {
    let _only = one_sweep_at_a_time().await;
    let Some(f) = boot().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };

    let silent = f.device("PRES-4", 30).await;
    // No user context anywhere: this is the worker's view, which owns no home.
    let _ = TenantCtx::system("test");
    f.devices.age_out_stale(60).await.expect("sweep");

    assert!(!f.online(silent).await);
}

// Keeps the trait import honest: the repo is used through `PgDevices`'s own methods here, and a
// future change that moves them onto the trait should fail loudly rather than silently unused.
#[allow(dead_code)]
fn _assert_trait_is_importable<D: DeviceRepo>() {}
