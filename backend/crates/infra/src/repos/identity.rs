use crate::pg::Pg;
use domain::identity::{IdentityRepo, RefreshRow, Session, User, UserTotp, VendorUser};
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

impl PgIdentity {
    /// Delete refresh rows that are long dead.
    ///
    /// Nothing ever removed them: every sign-in, every rotation and every sign-out left a row behind, so the table grew
    /// with the life of the account — and the sessions screen lists from exactly this table. Thirty days is longer than
    /// any token's own expiry, so this never deletes a row that could still be used to authenticate.
    pub async fn prune_sessions(&self, keep_days: i64) -> Result<u64, AppError> {
        let affected = sqlx::query(
            r#"DELETE FROM refresh_tokens
                WHERE expires_at < now() - ($1 || ' days')::interval
                   OR (revoked_at IS NOT NULL AND revoked_at < now() - ($1 || ' days')::interval)"#,
        )
        .bind(keep_days.to_string())
        .execute(self.pg.pool())
        .await
        .map_err(db)?
        .rows_affected();

        Ok(affected)
    }
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

    async fn active_sessions(&self, user_id: Uuid) -> Result<Vec<Session>, AppError> {
        // One row per family, not per token: a refresh token rotates on every use, so counting rows would report a
        // phone that has been open for a week as fifty sign-ins.
        let rows = sqlx::query_as::<_, (Uuid, time::OffsetDateTime, time::OffsetDateTime, time::OffsetDateTime)>(
            r#"SELECT family_id,
                      min(created_at)  AS started_at,
                      max(coalesce(used_at, created_at)) AS last_seen_at,
                      max(expires_at)  AS expires_at
                 FROM refresh_tokens
                WHERE user_id = $1
                  AND revoked_at IS NULL
                  AND expires_at > now()
                GROUP BY family_id
                ORDER BY started_at DESC
                -- Capped: a customer who signs in from a browser every day has hundreds of families, and a list of
                -- 856 rows answers nothing. The count in `active_sessions_count` is the number that is shown.
                LIMIT 20"#,
        )
        .bind(user_id)
        .fetch_all(self.pg.pool())
        .await
        .map_err(db)?;

        Ok(rows
            .into_iter()
            .map(
                |(family_id, started_at, last_seen_at, expires_at)| Session {
                    family_id,
                    started_at,
                    last_seen_at,
                    expires_at,
                },
            )
            .collect())
    }

    async fn start_totp(&self, user_id: Uuid, sealed_secret: &str) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("user-totp")).await?;
        // Re-enrolling replaces an open enrolment. A confirmed one is not kept: the person asked for a new seed, and a
        // row that holds two would be a second way in that nobody can see on a screen.
        sqlx::query(
            r#"INSERT INTO user_totp (user_id, sealed_secret, confirmed_at, last_step)
               VALUES ($1, $2, NULL, NULL)
               ON CONFLICT (user_id) DO UPDATE
                 SET sealed_secret = EXCLUDED.sealed_secret,
                     confirmed_at = NULL,
                     last_step = NULL,
                     created_at = now()"#,
        )
        .bind(user_id)
        .bind(sealed_secret)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn totp(&self, user_id: Uuid) -> Result<Option<UserTotp>, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("user-totp")).await?;
        let row = sqlx::query_as::<_, (String, Option<time::OffsetDateTime>, Option<i64>)>(
            "SELECT sealed_secret, confirmed_at, last_step FROM user_totp WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(
            row.map(|(sealed_secret, confirmed_at, last_step)| UserTotp {
                sealed_secret,
                confirmed_at,
                last_step,
            }),
        )
    }

    async fn confirm_totp(&self, user_id: Uuid) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("user-totp")).await?;
        let n = sqlx::query("UPDATE user_totp SET confirmed_at = now() WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        tx.commit().await.map_err(db)?;
        if n == 0 {
            return Err(AppError::not_found("enrolment"));
        }
        Ok(())
    }

    async fn spend_totp_step(&self, user_id: Uuid, step: i64) -> Result<bool, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("user-totp")).await?;
        // One statement, so two requests arriving in the same step cannot both win: the `WHERE` is the lock.
        let n = sqlx::query(
            "UPDATE user_totp SET last_step = $2 WHERE user_id = $1 AND (last_step IS NULL OR last_step < $2)",
        )
        .bind(user_id)
        .bind(step)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        tx.commit().await.map_err(db)?;
        Ok(n == 1)
    }

    async fn delete_totp(&self, user_id: Uuid) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("user-totp")).await?;
        sqlx::query("DELETE FROM user_totp WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn set_password_hash(&self, user_id: Uuid, hash: &str) -> Result<(), AppError> {
        let n = sqlx::query("UPDATE users SET password_hash = $2 WHERE id = $1")
            .bind(user_id)
            .bind(hash)
            .execute(self.pg.pool())
            .await
            .map_err(db)?
            .rows_affected();
        if n == 0 {
            return Err(AppError::not_found("account"));
        }
        Ok(())
    }

    async fn active_sessions_count(&self, user_id: Uuid) -> Result<i64, AppError> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT count(DISTINCT family_id)
                 FROM refresh_tokens
                WHERE user_id = $1 AND revoked_at IS NULL AND expires_at > now()"#,
        )
        .bind(user_id)
        .fetch_one(self.pg.pool())
        .await
        .map_err(db)?;
        Ok(count)
    }

    async fn revoke_all_except(&self, user_id: Uuid, keep: Option<Uuid>) -> Result<u64, AppError> {
        // `$2::uuid IS NULL` is "no exception", which is how "sign out everywhere" and "sign out the others" become one
        // statement instead of two that can drift apart.
        let affected = sqlx::query(
            r#"UPDATE refresh_tokens
                  SET revoked_at = now()
                WHERE user_id = $1
                  AND revoked_at IS NULL
                  AND ($2::uuid IS NULL OR family_id <> $2)"#,
        )
        .bind(user_id)
        .bind(keep)
        .execute(self.pg.pool())
        .await
        .map_err(db)?
        .rows_affected();

        Ok(affected)
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
