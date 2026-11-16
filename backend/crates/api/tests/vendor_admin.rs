//! The operator's console, against a real database.
//!
//! The properties that matter are the ones a query can get subtly wrong: a vendor list that misses
//! a vendor, a slug that collides, and — most of all — what suspension is allowed to do. `docs/11`
//! makes one promise in writing: new activations stop, existing devices keep working, the portal
//! goes read-only. A delete would break the second half of that promise, so these tests check the
//! promise rather than the endpoint.

use domain::pairing::{PairingRepo, SerialTake};
use domain::vendor_admin::{NewVendor, VendorAdminRepo, VendorPatch};
use kernel::ErrorCode;
use uuid::Uuid;

struct Fixture {
    pg: infra::Pg,
    db_url: String,
}

async fn fixture() -> Option<Fixture> {
    let db_url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pg = infra::Pg::connect(&db_url, 2).await.ok()?;
    Some(Fixture { pg, db_url })
}

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

fn vendor(slug: &str) -> NewVendor {
    NewVendor {
        slug: slug.into(),
        name: format!("Vendor {slug}"),
        quota_devices: Some(500),
    }
}

#[tokio::test]
async fn a_vendor_can_be_onboarded_and_appears_in_the_list() {
    let Some(f) = fixture().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    let repo = infra::PgVendorAdmin::new(f.pg.clone());
    let slug = format!("new-{}", &Uuid::now_v7().to_string()[20..32].to_lowercase());

    let created = repo.create(&vendor(&slug)).await.expect("create");
    assert_eq!(created.slug, slug);
    assert_eq!(created.status, "active");
    assert_eq!(created.quota_devices, Some(500));
    assert_eq!(created.devices, 0);
    assert_eq!(created.granted, 0);

    let list = repo.list(200).await.expect("list");
    assert!(
        list.iter().any(|v| v.id == created.id),
        "a vendor just created must be in the list"
    );
}

#[tokio::test]
async fn two_vendors_cannot_share_a_slug() {
    let Some(f) = fixture().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    let repo = infra::PgVendorAdmin::new(f.pg.clone());
    let slug = format!("dup-{}", &Uuid::now_v7().to_string()[20..32].to_lowercase());

    repo.create(&vendor(&slug)).await.expect("first");
    let err = repo.create(&vendor(&slug)).await.unwrap_err();

    // A slug is a support handle and a URL: two tenants behind one is a support ticket nobody can
    // untangle later.
    assert_eq!(err.code_kind(), ErrorCode::Conflict);
}

#[tokio::test]
async fn suspension_stops_new_activations_and_leaves_existing_devices_alone() {
    let Some(f) = fixture().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    let pool = admin(&f).await;
    let repo = infra::PgVendorAdmin::new(f.pg.clone());
    let slug = format!("sus-{}", &Uuid::now_v7().to_string()[20..32].to_lowercase());
    let created = repo.create(&vendor(&slug)).await.expect("create");

    // A product, a licence and one already-activated device: the state a vendor is in when an
    // invoice dispute starts.
    let product_id = Uuid::now_v7();
    let device_id = Uuid::now_v7();
    let home_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let model = serde_json::json!({
        "schema": 1, "properties": {}, "actions": {},
        "constraints": { "allow_unknown_serial": false }
    });

    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'Home owner')")
        .bind(user_id)
        .bind(format!("owner-{user_id}@example.test"))
        .execute(&pool)
        .await
        .expect("user");
    sqlx::query("INSERT INTO homes (id, owner_id, name) VALUES ($1, $2, 'A home')")
        .bind(home_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .expect("home");
    sqlx::query(
        "INSERT INTO products (id, vendor_id, product_key, name, category, thing_model, status, certification)
         VALUES ($1, $2, $3, 'Suspend Test Bulb', 'light', $4, 'published', 'certified')",
    )
    .bind(product_id)
    .bind(created.id)
    // The tail, not the head: a v7 uuid's leading hex is the timestamp, so two tests
    // created in the same millisecond would collide on the product key.
    .bind(format!("prd_{}", &product_id.to_string()[24..36]))
    .bind(&model)
    .execute(&pool)
    .await
    .expect("product");
    sqlx::query(
        "INSERT INTO licence_grants (id, vendor_id, quantity, unit_price_cents, currency)
         VALUES ($1, $2, 10, 249, 'USD')",
    )
    .bind(Uuid::now_v7())
    .bind(created.id)
    .execute(&pool)
    .await
    .expect("grant");
    sqlx::query(
        "INSERT INTO devices (id, vendor_id, product_id, home_id, name, serial, secret_hash, product_key, category)
         VALUES ($1, $2, $3, $4, 'Already installed', 'SUS-1', 'x', $5, 'light')",
    )
    .bind(device_id)
    .bind(created.id)
    .bind(product_id)
    .bind(home_id)
    // The tail, not the head: a v7 uuid's leading hex is the timestamp, so two tests
    // created in the same millisecond would collide on the product key.
    .bind(format!("prd_{}", &product_id.to_string()[24..36]))
    .execute(&pool)
    .await
    .expect("device");

    // Suspend.
    let suspended = repo
        .patch(
            created.id,
            &VendorPatch {
                status: Some("suspended".into()),
                quota_devices: None,
                name: None,
            },
        )
        .await
        .expect("suspend");
    assert_eq!(suspended.status, "suspended");

    // A new activation is refused, and it says why.
    let pairing = infra::PgPairing::new(f.pg.clone());
    let take = pairing
        .take_serial(product_id, "SUS-2", false)
        .await
        .expect("take");
    assert_eq!(
        take,
        domain::pairing::SerialTake::VendorSuspended,
        "a suspended vendor must not activate a device"
    );

    // The device already in the home is untouched: same row, same vendor, still there.
    let still_there: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM devices WHERE id = $1")
        .bind(device_id)
        .fetch_optional(&pool)
        .await
        .expect("lookup");
    assert!(still_there.is_some(), "suspension must not delete a device");

    // And the status the auth layer reads is the one the operator set.
    assert_eq!(
        repo.status_of(created.id).await.expect("status"),
        "suspended"
    );

    // A serial waiting on the shelf for the resumed vendor.
    sqlx::query(
        "INSERT INTO product_serials (vendor_id, product_id, serial) VALUES ($1, $2, 'SUS-2')",
    )
    .bind(created.id)
    .bind(product_id)
    .execute(&pool)
    .await
    .expect("manifest");

    // Resuming puts it back: the same serial can be activated once the operator restores them.
    repo.patch(
        created.id,
        &VendorPatch {
            status: Some("active".into()),
            quota_devices: None,
            name: None,
        },
    )
    .await
    .expect("resume");

    let take = pairing
        .take_serial(product_id, "SUS-2", false)
        .await
        .expect("take after resume");
    assert_eq!(take, domain::pairing::SerialTake::Taken);
}

#[tokio::test]
async fn a_quota_can_be_set_and_cleared() {
    let Some(f) = fixture().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    let repo = infra::PgVendorAdmin::new(f.pg.clone());
    let slug = format!("quo-{}", &Uuid::now_v7().to_string()[20..32].to_lowercase());
    let created = repo.create(&vendor(&slug)).await.expect("create");

    let raised = repo
        .patch(
            created.id,
            &VendorPatch {
                status: None,
                quota_devices: Some(Some(5000)),
                name: None,
            },
        )
        .await
        .expect("raise");
    assert_eq!(raised.quota_devices, Some(5000));

    // `Some(None)` clears it; `None` would have left it alone, which is the difference the DTO
    // exists for.
    let cleared = repo
        .patch(
            created.id,
            &VendorPatch {
                status: None,
                quota_devices: Some(None),
                name: None,
            },
        )
        .await
        .expect("clear");
    assert_eq!(cleared.quota_devices, None);
}

#[tokio::test]
async fn a_deleted_vendor_is_not_managed_from_the_console() {
    let Some(f) = fixture().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    let repo = infra::PgVendorAdmin::new(f.pg.clone());
    let slug = format!("del-{}", &Uuid::now_v7().to_string()[20..32].to_lowercase());
    let created = repo.create(&vendor(&slug)).await.expect("create");

    let err = repo
        .patch(
            created.id,
            &VendorPatch {
                status: Some("deleted".into()),
                quota_devices: None,
                name: None,
            },
        )
        .await
        .unwrap_err();

    assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);
}
