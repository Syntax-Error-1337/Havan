use domain::identity::{IdentityRepo, RefreshRow, User};
use kernel::AppError;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PgIdentity {
    pool: PgPool,
}

impl PgIdentity {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
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
        .fetch_optional(&self.pool)
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
        .fetch_optional(&self.pool)
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
        .execute(&self.pool)
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
            r#"INSERT INTO refresh_tokens (id, user_id, family_id, hash, expires_at)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(row.id)
        .bind(row.user_id)
        .bind(row.family_id)
        .bind(&row.hash)
        .bind(row.expires_at)
        .execute(&self.pool)
        .await
        .map(|_| ())
        .map_err(db)
    }

    async fn find_refresh(&self, id: Uuid) -> Result<Option<RefreshRow>, AppError> {
        sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                Uuid,
                String,
                time::OffsetDateTime,
                Option<time::OffsetDateTime>,
                Option<time::OffsetDateTime>,
            ),
        >(
            r#"SELECT id, user_id, family_id, hash, expires_at, used_at, revoked_at
               FROM refresh_tokens WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
        .map(|row| {
            row.map(
                |(id, user_id, family_id, hash, expires_at, used_at, revoked_at)| RefreshRow {
                    id,
                    user_id,
                    family_id,
                    hash,
                    expires_at,
                    used_at,
                    revoked_at,
                },
            )
        })
    }

    async fn mark_used(&self, id: Uuid) -> Result<(), AppError> {
        sqlx::query("UPDATE refresh_tokens SET used_at = now() WHERE id = $1 AND used_at IS NULL")
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db)
    }

    async fn revoke_family(&self, family_id: Uuid) -> Result<(), AppError> {
        sqlx::query("UPDATE refresh_tokens SET revoked_at = now() WHERE family_id = $1 AND revoked_at IS NULL")
            .bind(family_id)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(db)
    }
}
