use crate::pg::Pg;
use domain::catalog::{CatalogRepo, Product};
use kernel::{AppError, TenantCtx};
use protocol::thing_model::ThingModel;
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
    i32,
    serde_json::Value,
    time::OffsetDateTime,
);

fn map(row: ProductRow) -> Result<Product, AppError> {
    let (id, vendor_id, product_key, name, category, status, schema_version, model, created_at) =
        row;
    let thing_model: ThingModel = serde_json::from_value(model)
        .map_err(|e| AppError::Internal(format!("stored thing model is invalid: {e}")))?;

    Ok(Product {
        id,
        vendor_id,
        product_key,
        name,
        category,
        status,
        schema_version,
        thing_model,
        created_at,
    })
}

const COLUMNS: &str =
    "id, vendor_id, product_key, name, category, status, schema_version, thing_model, created_at";

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
}
