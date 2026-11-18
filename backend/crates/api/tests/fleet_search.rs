//! Finding one device in a fleet.
//!
//! The fleet screen had no way to look a serial up, which is the first thing support does: a customer
//! reads the label, the vendor pastes it. These tests hold both halves of that promise at the layer
//! that owns the SQL — the search finds what a person types, and a typed `%` is a character rather than
//! "everything".

use domain::fleet::{FleetFilter, FleetRepo};
use infra::repos::fleet::PgFleet;
use sqlx::PgPool;
use uuid::Uuid;

fn filter(q: &str) -> FleetFilter {
    FleetFilter {
        product_key: None,
        q: Some(q.to_string()),
        state: None,
        cursor: None,
        limit: Some(50),
        sort: None,
        dir: None,
    }
}

/// The escaping is the interesting part, so it is tested without a database.
#[test]
fn the_term_is_trimmed_capped_and_escaped() {
    assert_eq!(filter("   ").query(), None, "whitespace is not a search");
    assert_eq!(filter(" HUGE-1 ").query().as_deref(), Some("HUGE-1"));
    assert_eq!(filter(&"x".repeat(200)).query().expect("capped").len(), 64);
    // A `%` from a keyboard is a percent sign. Left alone it would match the whole fleet.
    assert_eq!(filter("a%b").query().as_deref(), Some("a\\%b"));
    assert_eq!(filter("a_b").query().as_deref(), Some("a\\_b"));
    assert_eq!(filter("a\\b").query().as_deref(), Some("a\\\\b"));
}

struct Fixture {
    pool: PgPool,
    fleet: PgFleet,
    vendor: Uuid,
    other_vendor: Uuid,
    home: Uuid,
    product: Uuid,
    other_product: Uuid,
}

/// Skip (not fail) without a test database: this suite runs in CI with one and on a laptop without
/// the containers, and a test that fails for "no database" trains people to ignore it.
async fn fixture() -> Option<Fixture> {
    let url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pool = PgPool::connect(&url).await.ok()?;
    let vendor = Uuid::now_v7();
    let other_vendor = Uuid::now_v7();
    let product = Uuid::now_v7();
    let other_product = Uuid::now_v7();
    let home = Uuid::now_v7();

    for (id, slug, name) in [
        (
            vendor,
            format!("srch{}", &vendor.simple().to_string()[..8]),
            "Search vendor",
        ),
        (
            other_vendor,
            format!("othr{}", &other_vendor.simple().to_string()[..8]),
            "Other vendor",
        ),
    ] {
        sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(slug)
            .bind(name)
            .execute(&pool)
            .await
            .ok()?;
    }
    for (id, vendor_id, key) in [
        (product, vendor, "q_search"),
        (other_product, other_vendor, "q_other"),
    ] {
        sqlx::query(
            "INSERT INTO products (id, vendor_id, product_key, name, thing_model)
             VALUES ($1, $2, $3, 'Search Bulb', '{}'::jsonb)",
        )
        .bind(id)
        .bind(vendor_id)
        .bind(key)
        .execute(&pool)
        .await
        .ok()?;
    }
    sqlx::query("INSERT INTO homes (id, name, owner_id) VALUES ($1, 'Search home', $2)")
        .bind(home)
        .bind(Uuid::now_v7())
        .execute(&pool)
        .await
        .ok()?;

    let mut devices = Vec::new();
    for (serial, name, vendor_id, product_id, home_id) in [
        (
            "SEARCH-PASTE-001",
            "Lamp in the hall",
            vendor,
            product,
            Some(home),
        ),
        (
            "SEARCH-PASTE-002",
            "Kitchen bulb",
            vendor,
            product,
            Some(home),
        ),
        // Another vendor's device, with a serial this test will also search for.
        (
            "SEARCH-PASTE-003",
            "Someone else's bulb",
            other_vendor,
            other_product,
            None,
        ),
    ] {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO devices (id, vendor_id, product_id, product_key, name, serial, home_id, state)
             VALUES ($1, $2, $3, 'q_search', $4, $5, $6, 'claimed')",
        )
        .bind(id)
        .bind(vendor_id)
        .bind(product_id)
        .bind(name)
        .bind(serial)
        .bind(home_id)
        .execute(&pool)
        .await
        .ok()?;
        devices.push(id);
    }

    Some(Fixture {
        fleet: PgFleet::new(infra::Pg::connect(&url, 2).await.ok()?),
        pool,
        vendor,
        other_vendor,
        home,
        product,
        other_product,
    })
}

#[tokio::test]
async fn a_pasted_serial_finds_its_device() {
    let Some(f) = fixture().await else { return };

    let found = f
        .fleet
        .devices(f.vendor, &filter("SEARCH-PASTE-001"))
        .await
        .expect("search");
    assert_eq!(found.len(), 1, "one serial, one row");
    assert_eq!(found[0].serial, "SEARCH-PASTE-001");
}

#[tokio::test]
async fn a_name_fragment_finds_the_devices_that_carry_it() {
    let Some(f) = fixture().await else { return };

    // Names are what a person remembers when the label is not to hand.
    let found = f
        .fleet
        .devices(f.vendor, &filter("kitchen"))
        .await
        .expect("search");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "Kitchen bulb");
}

#[tokio::test]
async fn a_wildcard_is_a_character_and_the_fleet_is_the_vendors_own() {
    let Some(f) = fixture().await else { return };

    // `%` matches nothing, rather than everything: the filter escapes it before it reaches `ILIKE`.
    let wildcard = f
        .fleet
        .devices(f.vendor, &filter("%"))
        .await
        .expect("search");
    assert!(wildcard.is_empty(), "a bare % must not return the fleet");

    // The other vendor's device carries the same prefix, and must stay invisible.
    let scoped = f
        .fleet
        .devices(f.vendor, &filter("SEARCH-PASTE"))
        .await
        .expect("search");
    assert_eq!(scoped.len(), 2, "two of this vendor's, none of the other's");
    assert!(scoped.iter().all(|d| d.serial != "SEARCH-PASTE-003"));

    // And their own door still works for them.
    let theirs = f
        .fleet
        .devices(f.other_vendor, &filter("SEARCH-PASTE-003"))
        .await
        .expect("search");
    assert_eq!(theirs.len(), 1);

    let _ = (f.home, f.product, f.other_product);
    sqlx::query("DELETE FROM devices WHERE vendor_id = ANY($1)")
        .bind(vec![f.vendor, f.other_vendor])
        .execute(&f.pool)
        .await
        .ok();
}
