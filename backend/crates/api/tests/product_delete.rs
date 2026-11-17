//! Removing a product from a vendor's catalogue, and the two things that make one permanent.
//!
//! A vendor with a mistaken draft had no way to delete it: the route did not exist at all. Adding it
//! is the easy half; the rules are the other half, and both of them protect something that is not the
//! vendor's convenience — devices people already own, and the activations an invoice was built from.
//! `licence_activations` cascades from `products`, so a naive delete would erase billing history.

use domain::catalog::{CatalogRepo, CatalogService};
use kernel::ErrorCode;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use uuid::Uuid;

struct Fixture {
    pg: sqlx::PgPool,
    service: CatalogService<infra::PgCatalog>,
    vendor_id: Uuid,
}

async fn boot() -> Option<Fixture> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db)
        .await
        .ok()?;
    let vendor_id = Uuid::now_v7();
    sqlx::query("INSERT INTO vendors (id, slug, name) VALUES ($1, $2, 'Delete Vendor')")
        .bind(vendor_id)
        .bind(format!("del-{}", &vendor_id.simple().to_string()[24..]))
        .execute(&admin)
        .await
        .ok()?;

    Some(Fixture {
        pg: admin,
        service: CatalogService::new(Arc::new(infra::PgCatalog::new(pg))),
        vendor_id,
    })
}

impl Fixture {
    async fn product(&self, tag: &str) -> Uuid {
        let id = Uuid::now_v7();
        let key = format!("del_{tag}_{}", &id.simple().to_string()[24..]);
        sqlx::query(
            "INSERT INTO products (id, vendor_id, product_key, name, category, thing_model)
             VALUES ($1, $2, $3, 'Delete Bulb', 'light', '{}'::jsonb)",
        )
        .bind(id)
        .bind(self.vendor_id)
        .bind(&key)
        .execute(&self.pg)
        .await
        .expect("product");
        id
    }

    async fn device_on(&self, product_id: Uuid, key: &str) {
        let device_id = Uuid::now_v7();
        let tail = device_id.simple().to_string();
        sqlx::query(
            "INSERT INTO devices (id, vendor_id, product_id, product_key, name, serial, secret_hash,
                                  state, category)
             VALUES ($1, $2, $3, $4, 'Delete Bulb', $5, 'x', 'claimed', 'light')",
        )
        .bind(device_id)
        .bind(self.vendor_id)
        .bind(product_id)
        .bind(key)
        .bind(format!("DEL-{}", tail[tail.len() - 8..].to_uppercase()))
        .execute(&self.pg)
        .await
        .expect("device");
    }

    async fn activation_on(&self, product_id: Uuid, key: &str) {
        sqlx::query(
            "INSERT INTO licence_activations (id, vendor_id, product_id, serial, unit_price_cents, currency)
             VALUES ($1, $2, $3, $4, 249, 'USD')",
        )
        .bind(Uuid::now_v7())
        .bind(self.vendor_id)
        .bind(product_id)
        .bind(key)
        .execute(&self.pg)
        .await
        .expect("activation");
    }

    async fn exists(&self, product_id: Uuid) -> bool {
        let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM products WHERE id = $1")
            .bind(product_id)
            .fetch_one(&self.pg)
            .await
            .expect("count");
        n == 1
    }
}

#[tokio::test]
async fn an_unused_product_is_removed() {
    let Some(f) = boot().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    let product = f.product("unused").await;

    f.service
        .delete(f.vendor_id, product)
        .await
        .expect("delete an unused product");
    assert!(!f.exists(product).await, "the row is gone");
}

#[tokio::test]
async fn a_product_with_devices_in_homes_stays() {
    let Some(f) = boot().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    let product = f.product("lived").await;
    f.device_on(product, "del_lived").await;

    let err = f.service.delete(f.vendor_id, product).await.unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::Conflict);
    assert!(
        err.to_string().contains("live on this product"),
        "the refusal names the reason: {err}"
    );
    assert!(f.exists(product).await, "nothing was deleted");
}

#[tokio::test]
async fn a_product_that_has_been_billed_for_stays() {
    let Some(f) = boot().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    let product = f.product("billed").await;
    f.activation_on(product, "DEL-BILLED-1").await;

    let err = f.service.delete(f.vendor_id, product).await.unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::Conflict);
    assert!(
        err.to_string().contains("billing history"),
        "the refusal says why the ledger wins: {err}"
    );

    // The activation survived — the point of refusing, since the foreign key cascades.
    let (n,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM licence_activations WHERE product_id = $1")
            .bind(product)
            .fetch_one(&f.pg)
            .await
            .expect("count");
    assert_eq!(n, 1, "billing history must outlive the catalogue entry");
}

#[tokio::test]
async fn another_vendors_product_cannot_be_deleted() {
    let Some(f) = boot().await else {
        eprintln!("skipping: no TEST_DATABASE_URL");
        return;
    };
    let product = f.product("theirs").await;
    let stranger = Uuid::now_v7();

    let err = f.service.delete(stranger, product).await.unwrap_err();
    assert_eq!(err.code_kind(), ErrorCode::NotFound);
    assert!(f.exists(product).await, "the owner's product is untouched");
}

// Keeps the trait import honest: the repo is used through the service in these tests.
#[allow(dead_code)]
fn _repo_is_reachable<R: CatalogRepo>() {}
