//! Postgres access.
//!
//! Tenancy is enforced by row-level security, which means the *session* must
//! carry the tenant. [`Pg::scoped`] is the only way to get a transaction, and it
//! sets `haven.vendor_id` transaction-locally, so a request that forgets to
//! filter gets zero rows instead of another vendor's fleet.

use kernel::{AppError, TenantCtx};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, Transaction};
use std::time::Duration;

#[derive(Clone)]
pub struct Pg {
    pool: PgPool,
}

impl Pg {
    pub async fn connect(url: &str, max_connections: u32) -> Result<Self, AppError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .idle_timeout(Some(Duration::from_secs(600)))
            .test_before_acquire(true)
            .connect(url)
            .await
            .map_err(|e| AppError::Database(Box::new(e)))?;
        Ok(Self { pool })
    }

    /// For tests and the migrate binary, which need the raw pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn ping(&self) -> bool {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .is_ok()
    }

    /// A transaction whose session carries the tenant. Every tenant-scoped
    /// query in the workspace goes through here.
    pub async fn scoped(
        &self,
        ctx: &TenantCtx,
    ) -> Result<Transaction<'static, Postgres>, AppError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Database(Box::new(e)))?;

        match ctx.vendor_id {
            Some(vendor_id) => {
                // `true` = transaction-local; it cannot leak to the next user of
                // this pooled connection.
                sqlx::query("SELECT set_config('haven.vendor_id', $1, true)")
                    .bind(vendor_id.to_string())
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::Database(Box::new(e)))?;
                sqlx::query("SELECT set_config('haven.actor', $1, true)")
                    .bind(actor_label(ctx))
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::Database(Box::new(e)))?;
            }
            None if ctx.is_system() => {
                // The only role that bypasses RLS. It exists so retention and
                // fan-out jobs can cross tenants; nothing user-facing uses it.
                sqlx::query("SET LOCAL ROLE haven_system")
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::Database(Box::new(e)))?;
            }
            // A customer app request carries no vendor; it is scoped by home
            // membership, the second policy on every device table.
            None if ctx.user_id().is_some() => {}
            None => {
                return Err(AppError::forbidden(
                    "a tenant context is required for this operation",
                ));
            }
        }

        // A customer app request carries no vendor: it is scoped by home
        // membership instead, which is the second policy on each device table.
        if let Some(user_id) = ctx.user_id() {
            sqlx::query("SELECT set_config('haven.user_id', $1, true)")
                .bind(user_id.to_string())
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Database(Box::new(e)))?;
        }

        Ok(tx)
    }
}

fn actor_label(ctx: &TenantCtx) -> String {
    use kernel::Actor::*;
    match &ctx.actor {
        User { user_id, .. } => format!("user:{user_id}"),
        ApiKey { key_id, .. } => format!("key:{key_id}"),
        Device { device_id } => format!("device:{device_id}"),
        System { job } => format!("system:{job}"),
        Anonymous { ip } => format!("anon:{}", ip.as_deref().unwrap_or("-")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url() -> Option<String> {
        std::env::var("TEST_DATABASE_URL").ok()
    }

    #[tokio::test]
    async fn system_context_may_cross_tenants_but_anonymous_may_not() {
        let Some(url) = url() else {
            eprintln!("skipping: TEST_DATABASE_URL not set");
            return;
        };
        let pg = Pg::connect(&url, 2).await.expect("connect");

        assert!(pg.scoped(&TenantCtx::system("test")).await.is_ok());

        let anon = TenantCtx {
            vendor_id: None,
            actor: kernel::Actor::Anonymous { ip: None },
            request_id: "t".into(),
        };
        assert!(pg.scoped(&anon).await.is_err());
    }
}
