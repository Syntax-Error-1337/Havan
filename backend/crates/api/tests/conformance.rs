//! Conformance: the lab a vendor's suite runs in, and the guard that keeps it a lab.
//!
//! These run against the real database because the properties are database properties: the serial
//! is a row the claim will look for, the licence is a balance computed from rows, and the guard is a
//! join between a device and its home's name. A fake would let all three be true in memory and false
//! in production.

use infra::repos::conformance::PgConformance;
use kernel::ErrorCode;
use uuid::Uuid;

struct Fixture {
    pg: infra::Pg,
    db_url: String,
    vendor_id: Uuid,
    product_id: Uuid,
}

async fn fixture() -> Option<Fixture> {
    let db_url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pg = infra::Pg::connect(&db_url, 2).await.ok()?;
    Some(Fixture {
        pg,
        db_url,
        vendor_id: Uuid::now_v7(),
        product_id: Uuid::now_v7(),
    })
}

/// `FORCE ROW LEVEL SECURITY` refuses a plain connection, which is the behaviour under test, so
/// fixtures seed as the role the API itself uses.
async fn admin(f: &Fixture) -> sqlx::PgPool {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&f.db_url)
        .await
        .expect("admin pool");
    sqlx::query("SET ROLE haven_system")
        .execute(&pool)
        .await
        .expect("system role");
    pool
}

async fn seed(f: &Fixture, allow_unknown_serial: bool) {
    let admin = admin(f).await;

    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, $3)")
        .bind(f.vendor_id)
        .bind(format!("conf-{}", &f.vendor_id.to_string()[20..32]))
        .bind("Conformance Test Vendor")
        .execute(&admin)
        .await
        .expect("vendor");

    let model = serde_json::json!({
        "schema": 1,
        "properties": {},
        "actions": {},
        "constraints": { "allow_unknown_serial": allow_unknown_serial }
    });
    sqlx::query(
        "INSERT INTO products (id, vendor_id, product_key, name, category, thing_model, status, certification)
         VALUES ($1, $2, $3, $4, 'light', $5, 'published', 'certified')",
    )
    .bind(f.product_id)
    .bind(f.vendor_id)
    .bind(format!("prd-{}", &f.product_id.to_string()[20..32]))
    .bind("Conformance Test Bulb")
    .bind(&model)
    .execute(&admin)
    .await
    .expect("product");
}

async fn grant(f: &Fixture, quantity: i32) {
    let admin = admin(f).await;
    sqlx::query(
        "INSERT INTO licence_grants (id, vendor_id, quantity, unit_price_cents, currency)
         VALUES ($1, $2, $3, 249, 'USD')",
    )
    .bind(Uuid::now_v7())
    .bind(f.vendor_id)
    .bind(quantity)
    .execute(&admin)
    .await
    .expect("grant");
}

#[tokio::test]
async fn a_session_prepares_a_serial_the_platform_will_look_for() {
    let Some(f) = fixture().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    seed(&f, false).await;
    grant(&f, 10).await;

    let repo = PgConformance::new(f.pg.clone());
    let material = repo
        .prepare(f.vendor_id, f.product_id)
        .await
        .expect("prepare");

    assert!(material.serial.starts_with("CONF-"), "{}", material.serial);
    assert_eq!(
        material.product_key,
        format!("prd-{}", &f.product_id.to_string()[20..32])
    );
    assert!(!material.allow_unknown_serial);
    // 64 hex characters, the same shape a factory provisioner writes into a device.
    assert_eq!(material.secret.len(), 64);
    assert!(material.secret.chars().all(|c| c.is_ascii_hexdigit()));

    let admin = admin(&f).await;
    let row: Option<(Option<time::OffsetDateTime>,)> = sqlx::query_as(
        "SELECT claimed_at FROM product_serials WHERE product_id = $1 AND serial = $2",
    )
    .bind(f.product_id)
    .bind(&material.serial)
    .fetch_optional(&admin)
    .await
    .expect("serial lookup");

    // The serial exists and is unclaimed: a manifest row, waiting for the claim the run will make.
    assert!(
        matches!(row, Some((None,))),
        "expected an open serial row: {row:?}"
    );

    let home: (String,) = sqlx::query_as("SELECT name FROM homes WHERE id = $1")
        .bind(material.home_id)
        .fetch_one(&admin)
        .await
        .expect("lab home");
    assert_eq!(home.0, "Conformance lab");
}

#[tokio::test]
async fn two_runs_never_share_a_serial() {
    let Some(f) = fixture().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    seed(&f, false).await;
    grant(&f, 10).await;

    let repo = PgConformance::new(f.pg.clone());
    let first = repo
        .prepare(f.vendor_id, f.product_id)
        .await
        .expect("first");
    let second = repo
        .prepare(f.vendor_id, f.product_id)
        .await
        .expect("second");

    // A conformance device is claimed by its run, so a reused serial would fail on its secret the
    // second time — which looks like a platform bug and is the platform being right.
    assert_ne!(first.serial, second.serial);
    assert_ne!(first.secret, second.secret);
    assert_eq!(
        first.home_id, second.home_id,
        "the lab is one home, not one per run"
    );
}

#[tokio::test]
async fn a_run_without_licences_is_refused_rather_than_allowed_on_credit() {
    let Some(f) = fixture().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    seed(&f, false).await;

    let repo = PgConformance::new(f.pg.clone());
    let err = repo.prepare(f.vendor_id, f.product_id).await.unwrap_err();

    // 402, not 403: a conformance device is an activated device, and the vendor is short of
    // licences rather than forbidden from testing.
    assert_eq!(err.code_kind(), ErrorCode::LicenceExhausted);

    grant(&f, 1).await;
    repo.prepare(f.vendor_id, f.product_id)
        .await
        .expect("one licence is enough to run");
}

#[tokio::test]
async fn the_command_door_refuses_a_device_outside_the_lab() {
    let Some(f) = fixture().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    seed(&f, false).await;
    grant(&f, 10).await;

    let repo = PgConformance::new(f.pg.clone());
    let material = repo
        .prepare(f.vendor_id, f.product_id)
        .await
        .expect("prepare");
    let admin = admin(&f).await;

    // A device in the lab, standing in for the one a run would claim.
    let lab_device = Uuid::now_v7();
    insert_device(&admin, &f, lab_device, material.home_id).await;

    assert_eq!(
        repo.lab_owner(f.vendor_id, lab_device)
            .await
            .expect("lab owner"),
        material.user_id
    );

    // The same device moved into a customer's home: an operator token must not reach it. The home
    // is owned by an ordinary person, which is what makes it a customer's rather than the lab's.
    let customer = Uuid::now_v7();
    let customer_home = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'A customer')")
        .bind(customer)
        .bind(format!("customer-{customer}@example.test"))
        .execute(&admin)
        .await
        .expect("customer");
    sqlx::query("INSERT INTO homes (id, owner_id, name) VALUES ($1, $2, 'A customer home')")
        .bind(customer_home)
        .bind(customer)
        .execute(&admin)
        .await
        .expect("customer home");
    sqlx::query("UPDATE devices SET home_id = $2 WHERE id = $1")
        .bind(lab_device)
        .bind(customer_home)
        .execute(&admin)
        .await
        .expect("move device");

    let err = repo.lab_owner(f.vendor_id, lab_device).await.unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::Forbidden);

    // And a device that does not exist is not found, rather than forbidden-by-accident.
    let err = repo
        .lab_owner(f.vendor_id, Uuid::now_v7())
        .await
        .unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::NotFound);
}

async fn insert_device(admin: &sqlx::PgPool, f: &Fixture, id: Uuid, home_id: Uuid) {
    sqlx::query(
        "INSERT INTO devices (id, vendor_id, product_id, home_id, name, serial, secret_hash,
                              product_key, category)
         VALUES ($1, $2, $3, $4, 'Conformance device', $5, 'x', $6, 'light')",
    )
    .bind(id)
    .bind(f.vendor_id)
    .bind(f.product_id)
    .bind(home_id)
    .bind(format!("CONF-{}", id.to_string()[..8].to_uppercase()))
    .bind(format!("prd-{}", &f.product_id.to_string()[20..32]))
    .execute(admin)
    .await
    .expect("device");
}
