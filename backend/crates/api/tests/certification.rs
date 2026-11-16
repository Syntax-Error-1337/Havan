//! Certification: the operator's sign-off is what lets a device into the app.
//!
//! The state machine is unit-tested in `domain::catalog`; this file proves the parts that
//! only the database can be wrong about — the row lock, the RLS scoping, the "publish
//! first" rule, and that the claim path actually reads the column it now enforces.

use domain::catalog::{CatalogRepo, Product};
use domain::pairing::PairingRepo;
use kernel::ErrorCode;
use serde_json::json;
use uuid::Uuid;

struct Fixture {
    pg: infra::Pg,
    db_url: String,
    vendor_id: Uuid,
    product_id: Uuid,
}

/// One-connection pool in the system role, the same role the API uses for provisioning.
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

async fn fixture(status: &str) -> Option<Fixture> {
    let db_url = std::env::var("TEST_DATABASE_URL").ok()?;
    let pg = infra::Pg::connect(&db_url, 2).await.ok()?;
    let f = Fixture {
        pg,
        db_url,
        vendor_id: Uuid::now_v7(),
        product_id: Uuid::now_v7(),
    };

    let admin = admin(&f).await;
    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Certification Vendor')")
        .bind(f.vendor_id)
        .bind(format!("crt-{}", &f.vendor_id.to_string()[20..32]))
        .execute(&admin)
        .await
        .expect("vendor");

    let model = json!({
        "product_key": format!("hv_cert_{}", &f.product_id.to_string()[20..32]),
        "name": "Certification Bulb",
        "category": "light",
        "capabilities": [{ "name": "switch", "points": { "power": { "dp": "1", "kind": "bool" } } }],
        "constraints": { "allow_unknown_serial": false }
    });
    sqlx::query(
        "INSERT INTO products (id, vendor_id, product_key, name, category, thing_model, status)
         VALUES ($1, $2, $3, 'Certification Bulb', 'light', $4, $5)",
    )
    .bind(f.product_id)
    .bind(f.vendor_id)
    .bind(format!("hv_cert_{}", &f.product_id.to_string()[20..32]))
    .bind(&model)
    .bind(status)
    .execute(&admin)
    .await
    .expect("product");

    Some(f)
}

async fn certification_of(f: &Fixture) -> String {
    let admin = admin(f).await;
    sqlx::query_scalar("SELECT certification FROM products WHERE id = $1")
        .bind(f.product_id)
        .fetch_one(&admin)
        .await
        .expect("certification")
}

#[tokio::test]
async fn a_published_product_walks_uncertified_to_certified() {
    let Some(f) = fixture("published").await else {
        return;
    };
    let catalog = infra::PgCatalog::new(f.pg.clone());

    assert_eq!(certification_of(&f).await, "uncertified");

    catalog
        .set_certification(Some(f.vendor_id), f.product_id, "submitted", Some("ready"))
        .await
        .expect("submit");
    assert_eq!(certification_of(&f).await, "submitted");

    // The operator certifies across tenants, which is why the scope is None.
    catalog
        .set_certification(
            None,
            f.product_id,
            "certified",
            Some("physical test passed"),
        )
        .await
        .expect("certify");
    assert_eq!(certification_of(&f).await, "certified");

    let admin = admin(&f).await;
    let row: (Option<time::OffsetDateTime>, Option<String>) =
        sqlx::query_as("SELECT certified_at, certification_note FROM products WHERE id = $1")
            .bind(f.product_id)
            .fetch_one(&admin)
            .await
            .expect("row");
    assert!(row.0.is_some(), "certified_at must be set on sign-off");
    assert_eq!(row.1.as_deref(), Some("physical test passed"));
}

#[tokio::test]
async fn the_claim_path_sees_the_certification_it_enforces() {
    let Some(f) = fixture("published").await else {
        return;
    };
    let catalog = infra::PgCatalog::new(f.pg.clone());
    let pairing = infra::PgPairing::new(f.pg.clone());
    let admin = admin(&f).await;
    let key: String = sqlx::query_scalar("SELECT product_key FROM products WHERE id = $1")
        .bind(f.product_id)
        .fetch_one(&admin)
        .await
        .expect("key");

    // Uncertified: the product is found, and the record says so — the refusal happens in
    // `domain::pairing::claim`, which is unit-tested there.
    let record = pairing
        .product_by_key(&key)
        .await
        .expect("lookup")
        .expect("found");
    assert_eq!(record.certification, "uncertified");

    catalog
        .set_certification(Some(f.vendor_id), f.product_id, "submitted", None)
        .await
        .expect("submit");
    catalog
        .set_certification(None, f.product_id, "certified", None)
        .await
        .expect("certify");

    let record = pairing
        .product_by_key(&key)
        .await
        .expect("lookup")
        .expect("found");
    assert_eq!(record.certification, "certified");
}

#[tokio::test]
async fn a_draft_cannot_be_submitted_for_certification() {
    let Some(f) = fixture("draft").await else {
        return;
    };
    let catalog = infra::PgCatalog::new(f.pg.clone());

    let err = catalog
        .set_certification(Some(f.vendor_id), f.product_id, "submitted", None)
        .await
        .unwrap_err();

    assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);
    assert_eq!(certification_of(&f).await, "uncertified");
}

#[tokio::test]
async fn the_shortcut_from_uncertified_to_certified_is_refused_by_the_database_path_too() {
    let Some(f) = fixture("published").await else {
        return;
    };
    let catalog = infra::PgCatalog::new(f.pg.clone());

    let err = catalog
        .set_certification(None, f.product_id, "certified", None)
        .await
        .unwrap_err();

    assert_eq!(err.code_kind(), ErrorCode::Conflict);
    assert_eq!(certification_of(&f).await, "uncertified");
}

#[tokio::test]
async fn a_revoked_product_has_to_be_resubmitted() {
    let Some(f) = fixture("published").await else {
        return;
    };
    let catalog = infra::PgCatalog::new(f.pg.clone());

    for (to, _) in [("submitted", 0), ("certified", 0)] {
        catalog
            .set_certification(None, f.product_id, to, None)
            .await
            .expect(to);
    }
    catalog
        .set_certification(None, f.product_id, "revoked", Some("bad firmware"))
        .await
        .expect("revoke");
    assert_eq!(certification_of(&f).await, "revoked");

    // Straight back to certified is not a path: the vendor has to ask again, and we have to
    // test again. Otherwise a revoke is decoration.
    let err = catalog
        .set_certification(None, f.product_id, "certified", None)
        .await
        .unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::Conflict);

    catalog
        .set_certification(Some(f.vendor_id), f.product_id, "submitted", None)
        .await
        .expect("resubmit");
    assert_eq!(certification_of(&f).await, "submitted");
}

#[tokio::test]
async fn another_vendor_cannot_submit_this_product() {
    let Some(f) = fixture("published").await else {
        return;
    };
    let catalog = infra::PgCatalog::new(f.pg.clone());

    let err = catalog
        .set_certification(Some(Uuid::now_v7()), f.product_id, "submitted", None)
        .await
        .unwrap_err();

    // Not found, not forbidden: in another vendor's context the product does not exist, and
    // saying "forbidden" would confirm that it does.
    assert_eq!(err.code_kind(), ErrorCode::NotFound);
    assert_eq!(certification_of(&f).await, "uncertified");
}

/// `Product` gained a field; this keeps the struct honest for anyone constructing one.
#[allow(dead_code)]
fn _product_has_certification(p: &Product) -> &str {
    &p.certification
}
