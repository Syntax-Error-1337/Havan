use domain::api_key::{ApiKey, ApiKeyRepo, NewKey, matches};
use kernel::{AppError, TenantCtx};
use uuid::Uuid;

use crate::pg::Pg;

#[derive(Clone)]
pub struct PgApiKeys {
    pg: Pg,
}

impl PgApiKeys {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    async fn tx(
        &self,
        vendor_id: Uuid,
    ) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, AppError> {
        self.pg
            .scoped(&TenantCtx {
                vendor_id: Some(vendor_id),
                actor: kernel::Actor::ApiKey {
                    vendor_id,
                    key_id: Uuid::nil(),
                },
                request_id: "api-keys".into(),
            })
            .await
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

type KeyRow = (
    Uuid,
    String,
    String,
    time::OffsetDateTime,
    Option<time::OffsetDateTime>,
    Option<time::OffsetDateTime>,
);

fn map(row: KeyRow) -> ApiKey {
    let (id, name, prefix, created_at, last_used_at, revoked_at) = row;
    ApiKey {
        id,
        name,
        prefix,
        secret: None,
        created_at,
        last_used_at,
        revoked_at,
    }
}

impl ApiKeyRepo for PgApiKeys {
    async fn create(&self, vendor_id: Uuid, key: &NewKey) -> Result<(), AppError> {
        let mut tx = self.tx(vendor_id).await?;
        sqlx::query(
            r#"INSERT INTO api_keys (id, vendor_id, name, prefix, hash, scopes)
               VALUES ($1, $2, $3, $4, $5, '{vendor}')"#,
        )
        .bind(key.id)
        .bind(vendor_id)
        .bind(&key.name)
        .bind(&key.prefix)
        .bind(&key.hash)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn list(&self, vendor_id: Uuid) -> Result<Vec<ApiKey>, AppError> {
        let mut tx = self.tx(vendor_id).await?;
        let rows = sqlx::query_as::<_, KeyRow>(
            r#"SELECT id, name, prefix, created_at, last_used_at, revoked_at
                 FROM api_keys ORDER BY created_at DESC"#,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(rows.into_iter().map(map).collect())
    }

    async fn revoke(&self, vendor_id: Uuid, id: Uuid) -> Result<(), AppError> {
        let mut tx = self.tx(vendor_id).await?;
        let n = sqlx::query(
            "UPDATE api_keys SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        tx.commit().await.map_err(db)?;

        if n == 0 {
            return Err(AppError::not_found("api key"));
        }
        Ok(())
    }

    /// System-scoped: this runs before any vendor is known, because working out
    /// the vendor is the whole point. The query is keyed by an exact prefix.
    async fn authenticate(&self, prefix: &str, secret: &str) -> Result<Option<Uuid>, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("api-key-auth")).await?;
        let row = sqlx::query_as::<_, (Uuid, Uuid, String)>(
            "SELECT id, vendor_id, hash FROM api_keys WHERE prefix = $1 AND revoked_at IS NULL",
        )
        .bind(prefix)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        let Some((id, vendor_id, hash)) = row else {
            tx.commit().await.map_err(db)?;
            return Ok(None);
        };

        if !matches(secret, &hash) {
            tx.commit().await.map_err(db)?;
            return Ok(None);
        }

        sqlx::query("UPDATE api_keys SET last_used_at = now() WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(Some(vendor_id))
    }
}
