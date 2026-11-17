use crate::pg::Pg;
use domain::identity::{IdentityRepo, RefreshRow, User, VendorUser};
use kernel::{AppError, TenantCtx};
use uuid::Uuid;

pub struct PgIdentity {
    pg: Pg,
}

impl PgIdentity {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    fn pool(&self) -> &sqlx::PgPool {
        self.pg.pool()
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

impl IdentityRepo for PgIdentity {
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, AppError> {
        sqlx::query_as::<
            _,
            (
                Uuid,
                Option<String>,
                Option<String>,
                String,
                Option<String>,
                String,
            ),
        >(
            r#"SELECT id, email, phone, display_name, password_hash, locale
               FROM users WHERE lower(email) = lower($1) AND disabled_at IS NULL"#,
        )
        .bind(email)
        .fetch_optional(self.pool())
        .await
        .map_err(db)
        .map(|row| {
            row.map(
                |(id, email, phone, display_name, password_hash, locale)| User {
                    id,
                    email,
                    phone,
                    display_name,
                    password_hash,
                    locale,
                },
            )
        })
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, AppError> {
        sqlx::query_as::<
            _,
            (
                Uuid,
                Option<String>,
                Option<String>,
                String,
                Option<String>,
                String,
            ),
        >(
            r#"SELECT id, email, phone, display_name, password_hash, locale
               FROM users WHERE id = $1 AND disabled_at IS NULL"#,
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(db)
        .map(|row| {
            row.map(
                |(id, email, phone, display_name, password_hash, locale)| User {
                    id,
                    email,
                    phone,
                    display_name,
                    password_hash,
                    locale,
                },
            )
        })
    }

    async fn insert_user(&self, user: &User) -> Result<(), AppError> {
        sqlx::query(
            r#"INSERT INTO users (id, email, phone, display_name, password_hash, locale)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(user.id)
        .bind(&user.email)
        .bind(&user.phone)
        .bind(&user.display_name)
        .bind(&user.password_hash)
        .bind(&user.locale)
        .execute(self.pool())
        .await
        .map(|_| ())
        .map_err(|e| {
            // The unique index is the race-safe check behind the service's
            // "already exists" query.
            if e.as_database_error()
                .is_some_and(|d| d.code().as_deref() == Some("23505"))
            {
                AppError::code(
                    kernel::ErrorCode::Conflict,
                    "an account with that email already exists",
                )
            } else {
                db(e)
            }
        })
    }

    async fn insert_refresh(&self, row: &RefreshRow) -> Result<(), AppError> {
        sqlx::query(
            r#"INSERT INTO refresh_tokens
                 (id, user_id, vendor_user_id, family_id, vendor_id, hash, expires_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(row.id)
        .bind(row.user_id)
        .bind(row.vendor_user_id)
        .bind(row.family_id)
        .bind(row.vendor_id)
        .bind(&row.hash)
        .bind(row.expires_at)
        .execute(self.pool())
        .await
        .map(|_| ())
        .map_err(db)
    }

    async fn find_refresh(&self, id: Uuid) -> Result<Option<RefreshRow>, AppError> {
        sqlx::query_as::<
            _,
            (
                Uuid,
                Option<Uuid>,
                Option<Uuid>,
                Uuid,
                Option<Uuid>,
                String,
                time::OffsetDateTime,
                Option<time::OffsetDateTime>,
                Option<time::OffsetDateTime>,
            ),
        >(
            r#"SELECT id, user_id, vendor_user_id, family_id, vendor_id, hash,
                      expires_at, used_at, revoked_at
               FROM refresh_tokens WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(db)
        .map(|row| {
            row.map(
                |(
                    id,
                    user_id,
                    vendor_user_id,
                    family_id,
                    vendor_id,
                    hash,
                    expires_at,
                    used_at,
                    revoked_at,
                )| {
                    RefreshRow {
                        id,
                        user_id,
                        vendor_user_id,
                        vendor_id,
                        family_id,
                        hash,
                        expires_at,
                        used_at,
                        revoked_at,
                    }
                },
            )
        })
    }

    async fn mark_used(&self, id: Uuid) -> Result<(), AppError> {
        sqlx::query("UPDATE refresh_tokens SET used_at = now() WHERE id = $1 AND used_at IS NULL")
            .bind(id)
            .execute(self.pool())
            .await
            .map(|_| ())
            .map_err(db)
    }

    async fn vendor_totp(&self, user_id: Uuid) -> Result<Option<(String, bool)>, AppError> {
        let mut tx = self
            .pg
            .scoped(&TenantCtx::system("vendor-login-totp"))
            .await?;
        let row: Option<(Option<String>, bool)> = sqlx::query_as(
            "SELECT totp_secret, totp_confirmed_at IS NOT NULL
               FROM vendor_users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(Box::new(e)))?;

        // A secret that was never confirmed is not a factor: it is an enrolment somebody abandoned
        // halfway, and treating it as one would lock them out of an account they can still use.
        Ok(match row {
            Some((Some(secret), confirmed)) => Some((secret, confirmed)),
            _ => None,
        })
    }

    async fn touch_vendor_login(&self, user_id: Uuid) -> Result<(), AppError> {
        // System scope: authentication runs before a tenant exists, and this is the row that was
        // just authenticated, so there is nothing to enumerate.
        let mut tx = self
            .pg
            .scoped(&TenantCtx::system("vendor-login-touch"))
            .await?;
        sqlx::query("UPDATE vendor_users SET last_login_at = now() WHERE id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(Box::new(e)))?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(Box::new(e)))
    }

    async fn vendor_id_by_slug(&self, slug: &str) -> Result<Option<Uuid>, AppError> {
        // Same narrow exception as `find_vendor_user`: authentication happens before a tenant exists, so
        // this one lookup runs system-scoped and returns an id and nothing else.
        let mut tx = self.pg.scoped(&TenantCtx::system("vendor-login")).await?;
        let row: Option<(Uuid,)> =
            sqlx::query_as("SELECT id FROM vendors WHERE slug = $1 AND deleted_at IS NULL")
                .bind(slug)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        Ok(row.map(|r| r.0))
    }

    async fn find_vendor_user(
        &self,
        vendor_id: Uuid,
        email: &str,
    ) -> Result<Option<VendorUser>, AppError> {
        // Authentication runs before any tenant exists, and `vendor_users` is
        // vendor-scoped by RLS. This is the narrow, documented exception: one
        // lookup by (vendor, email), in a system-scoped transaction.
        let mut tx = self.pg.scoped(&TenantCtx::system("vendor-login")).await?;

        let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, String)>(
            r#"SELECT id, vendor_id, email, password_hash, role FROM vendor_users
               WHERE vendor_id = $1 AND lower(email) = lower($2)"#,
        )
        .bind(vendor_id)
        .bind(email)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(
            row.map(|(id, vendor_id, email, password_hash, role)| VendorUser {
                id,
                vendor_id,
                email,
                password_hash,
                role,
            }),
        )
    }

    async fn revoke_family(&self, family_id: Uuid) -> Result<(), AppError> {
        sqlx::query("UPDATE refresh_tokens SET revoked_at = now() WHERE family_id = $1 AND revoked_at IS NULL")
            .bind(family_id)
            .execute(self.pool())
            .await
            .map(|_| ())
            .map_err(db)
    }
}
