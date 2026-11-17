use crate::pg::Pg;
use domain::catalog::{CatalogRepo, Product, PublicProduct, Unused, certification_transition};
use kernel::{AppError, TenantCtx};
use protocol::thing_model::ThingModel;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Clone)]
pub struct PgCatalog {
    pg: Pg,
}

impl PgCatalog {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    fn ctx(vendor_id: Uuid) -> TenantCtx {
        TenantCtx {
            vendor_id: Some(vendor_id),
            actor: kernel::Actor::ApiKey {
                vendor_id,
                key_id: Uuid::nil(),
            },
            request_id: "catalog".into(),
        }
    }
}

/// The 4 bytes a device advertises: the first half of SHA-256(product_key), hex.
/// Both ends must agree on this, so it lives next to the query that serves it.
pub fn advertisement_hash(product_key: &str) -> String {
    let digest = Sha256::digest(product_key.as_bytes());
    digest[..4].iter().map(|b| format!("{b:02x}")).collect()
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

type ProductRow = (
    Uuid,
    Uuid,
    String,
    String,
    String,
    String,
    String,
    i32,
    serde_json::Value,
    time::OffsetDateTime,
);

fn map(row: ProductRow) -> Result<Product, AppError> {
    let (
        id,
        vendor_id,
        product_key,
        name,
        category,
        status,
        certification,
        schema_version,
        model,
        created_at,
    ) = row;
    let thing_model: ThingModel = serde_json::from_value(model)
        .map_err(|e| AppError::Internal(format!("stored thing model is invalid: {e}")))?;

    Ok(Product {
        id,
        vendor_id,
        product_key,
        name,
        category,
        status,
        certification,
        schema_version,
        thing_model,
        created_at,
    })
}

const COLUMNS: &str = "id, vendor_id, product_key, name, category, status, certification, \
     schema_version, thing_model, created_at";

impl CatalogRepo for PgCatalog {
    async fn insert(&self, vendor_id: Uuid, product: &Product) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&PgCatalog::ctx(vendor_id)).await?;

        let model = serde_json::to_value(&product.thing_model)
            .map_err(|e| AppError::Internal(format!("thing model: {e}")))?;

        sqlx::query(
            r#"INSERT INTO products
                 (id, vendor_id, product_key, name, category, status, schema_version,
                  thing_model, published_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,
                       CASE WHEN $6 = 'published' THEN now() ELSE NULL END)"#,
        )
        .bind(product.id)
        .bind(vendor_id)
        .bind(&product.product_key)
        .bind(&product.name)
        .bind(&product.category)
        .bind(&product.status)
        .bind(product.schema_version)
        .bind(&model)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if e.as_database_error()
                .is_some_and(|d| d.code().as_deref() == Some("23505"))
            {
                AppError::code(
                    kernel::ErrorCode::Conflict,
                    "that product_key is already taken",
                )
            } else {
                db(e)
            }
        })?;

        tx.commit().await.map_err(db)
    }

    async fn list(&self, vendor_id: Uuid) -> Result<Vec<Product>, AppError> {
        let mut tx = self.pg.scoped(&PgCatalog::ctx(vendor_id)).await?;
        let rows = sqlx::query_as::<_, ProductRow>(&format!(
            "SELECT {COLUMNS} FROM products ORDER BY created_at DESC"
        ))
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        rows.into_iter().map(map).collect()
    }

    async fn get(&self, vendor_id: Uuid, id: Uuid) -> Result<Option<Product>, AppError> {
        let mut tx = self.pg.scoped(&PgCatalog::ctx(vendor_id)).await?;
        let row = sqlx::query_as::<_, ProductRow>(&format!(
            "SELECT {COLUMNS} FROM products WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        row.map(map).transpose()
    }

    async fn update(&self, vendor_id: Uuid, product: &Product) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&PgCatalog::ctx(vendor_id)).await?;
        let model = serde_json::to_value(&product.thing_model)
            .map_err(|e| AppError::Internal(format!("thing model: {e}")))?;

        let n = sqlx::query(
            r#"UPDATE products
                  SET name = $2, category = $3, schema_version = $4, thing_model = $5
                WHERE id = $1 AND status <> 'published'"#,
        )
        .bind(product.id)
        .bind(&product.name)
        .bind(&product.category)
        .bind(product.schema_version)
        .bind(&model)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        tx.commit().await.map_err(db)?;

        if n == 0 {
            return Err(AppError::not_found("product"));
        }
        Ok(())
    }

    async fn set_certification(
        &self,
        vendor_id: Option<Uuid>,
        id: Uuid,
        to: &str,
        note: Option<&str>,
    ) -> Result<(), AppError> {
        // `None` is the operator, who acts across tenants; `Some` is a vendor on its own
        // product. The context is chosen here so the row lock and the RLS check happen in
        // the same transaction as the write.
        let ctx = match vendor_id {
            Some(vendor_id) => PgCatalog::ctx(vendor_id),
            None => TenantCtx::system("product-certification"),
        };
        let mut tx = self.pg.scoped(&ctx).await?;

        // Locked read: two operators clicking certify at once must not both see the old
        // state and write two different decisions.
        let current: Option<(String, String)> =
            sqlx::query_as("SELECT certification, status FROM products WHERE id = $1 FOR UPDATE")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;

        let Some((from, status)) = current else {
            return Err(AppError::not_found("product"));
        };

        // A draft cannot be submitted: a phone can only claim what is published, so
        // certifying a draft would sign off something nobody can reach.
        if to == "submitted" && status != "published" {
            return Err(AppError::validation(
                "publish the product before submitting it for certification",
            ));
        }

        certification_transition(&from, to)?;

        sqlx::query(
            r#"UPDATE products
                  SET certification = $2,
                      certified_at = CASE WHEN $2 = 'certified' THEN now() ELSE certified_at END,
                      certification_note = COALESCE($3, certification_note)
                WHERE id = $1"#,
        )
        .bind(id)
        .bind(to)
        .bind(note)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)
    }

    async fn delete(&self, vendor_id: Uuid, id: Uuid) -> Result<Option<Unused>, AppError> {
        let mut tx = self.pg.scoped(&PgCatalog::ctx(vendor_id)).await?;

        // One statement, so the counts and the delete cannot disagree: the row is removed only when
        // nothing points at it, and the same read says what did.
        let row: Option<(i64, i64, i64, i64)> = sqlx::query_as(
            r#"WITH counts AS (
                   SELECT p.id,
                          (SELECT count(*) FROM devices d
                            WHERE d.product_id = p.id AND d.state <> 'released') AS devices,
                          (SELECT count(*) FROM licence_activations a
                            WHERE a.product_id = p.id) AS activations
                     FROM products p
                    WHERE p.id = $2 AND p.vendor_id = $1
               ), gone AS (
                   DELETE FROM products p
                    USING counts c
                    WHERE p.id = c.id
                      AND c.devices = 0
                      AND c.activations = 0
                  RETURNING p.id
               )
               SELECT (SELECT count(*) FROM counts),
                      COALESCE((SELECT devices FROM counts), 0),
                      COALESCE((SELECT activations FROM counts), 0),
                      (SELECT count(*) FROM gone)"#,
        )
        .bind(vendor_id)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        match row {
            Some((0, _, _, _)) | None => Err(AppError::not_found("product")),
            Some((_, _, _, 1)) => Ok(None),
            Some((_, devices, activations, _)) => Ok(Some(Unused {
                devices,
                activations,
            })),
        }
    }

    async fn publish(&self, vendor_id: Uuid, id: Uuid) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&PgCatalog::ctx(vendor_id)).await?;
        let n = sqlx::query(
            "UPDATE products SET status = 'published', published_at = now() WHERE id = $1",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        tx.commit().await.map_err(db)?;

        if n == 0 {
            return Err(AppError::not_found("product"));
        }
        Ok(())
    }
    /// Runs system-scoped: a published product is public, and the caller is a
    /// phone with no vendor. Terms are exact matches, so this cannot enumerate
    /// a catalogue it was not asked about.
    async fn public_lookup(&self, terms: &[String]) -> Result<Vec<PublicProduct>, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("catalog-public")).await?;

        let rows = sqlx::query_as::<_, (String, String, String, Option<String>)>(
            r#"SELECT product_key, name, category, icon
                 FROM products
                WHERE status = 'published'"#,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        let wanted: Vec<String> = terms.iter().map(|t| t.to_lowercase()).collect();

        Ok(rows
            .into_iter()
            .map(|(product_key, name, category, icon)| PublicProduct {
                hash: advertisement_hash(&product_key),
                product_key,
                name,
                category,
                icon,
            })
            .filter(|p| wanted.contains(&p.hash) || wanted.contains(&p.product_key.to_lowercase()))
            .collect())
    }
}
