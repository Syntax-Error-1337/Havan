use domain::vendor_team::{
    INVITE_TTL_HOURS, PendingInvite, Role, Team, TeamMember, VendorTeamRepo,
};
use kernel::{AppError, ErrorCode, TenantCtx};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::pg::Pg;

/// The invitation token is a credential, so it is stored the way a password is: hashed, with the
/// plaintext handed to the caller exactly once. A backup that leaks should not be a stack of
/// working invitations.
fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Clone)]
pub struct PgVendorTeam {
    pg: Pg,
}

impl PgVendorTeam {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    async fn vendor_tx(
        &self,
        vendor_id: Uuid,
    ) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, AppError> {
        self.pg
            .scoped(&TenantCtx {
                vendor_id: Some(vendor_id),
                actor: kernel::Actor::User {
                    user_id: Uuid::nil(),
                    vendor_id: Some(vendor_id),
                },
                request_id: "vendor-team".into(),
            })
            .await
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

impl VendorTeamRepo for PgVendorTeam {
    async fn invite(
        &self,
        vendor_id: Uuid,
        invited_by: Uuid,
        email: &str,
        role: Role,
    ) -> Result<String, AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;

        // 32 bytes from the platform's id generator, hex-encoded: the database has no random
        // function we rely on, and a guessable invitation token is an account.
        let token = format!("{}{}", Uuid::now_v7().simple(), Uuid::now_v7().simple());
        let email = email.trim().to_ascii_lowercase();

        sqlx::query(
            r#"INSERT INTO vendor_invites (id, vendor_id, email, role, token_hash, invited_by, expires_at)
               VALUES ($1, $2, $3, $4, $5, $6, now() + make_interval(hours => $7))"#,
        )
        .bind(kernel::new_id())
        .bind(vendor_id)
        .bind(&email)
        .bind(role.as_str())
        .bind(hash_token(&token))
        .bind(invited_by)
        .bind(INVITE_TTL_HOURS as i32)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if e.as_database_error()
                .is_some_and(|d| d.code().as_deref() == Some("23505"))
            {
                AppError::code(
                    ErrorCode::Conflict,
                    "that address already has a live invitation; revoke it first",
                )
            } else {
                db(e)
            }
        })?;

        tx.commit().await.map_err(db)?;
        Ok(token)
    }

    async fn accept(
        &self,
        token: &str,
        display_name: &str,
        password: &str,
    ) -> Result<(Uuid, Uuid), AppError> {
        // No vendor context: the person accepting has no session yet — the token is the credential.
        let mut tx = self
            .pg
            .scoped(&TenantCtx::system("vendor-invite-accept"))
            .await?;

        let invite: Option<(
            Uuid,
            Uuid,
            String,
            String,
            Option<time::OffsetDateTime>,
            bool,
        )> = sqlx::query_as(
            r#"SELECT id, vendor_id, email, role, accepted_at, expires_at < now()
                     FROM vendor_invites WHERE token_hash = $1"#,
        )
        .bind(hash_token(token))
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        let Some((invite_id, vendor_id, email, role, accepted_at, expired)) = invite else {
            // The same answer for "no such invitation" and "wrong token": a token oracle is a way
            // to enumerate valid invitations.
            return Err(AppError::not_found("invitation"));
        };
        if accepted_at.is_some() {
            return Err(AppError::code(
                ErrorCode::Conflict,
                "that invitation has already been used",
            ));
        }
        if expired {
            return Err(AppError::code(
                ErrorCode::Conflict,
                "that invitation has expired; ask for a new one",
            ));
        }

        let user_id = kernel::new_id();
        let hash = domain::identity::hash_password(password)?;

        sqlx::query(
            r#"INSERT INTO vendor_users (id, vendor_id, email, display_name, role, password_hash)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(user_id)
        .bind(vendor_id)
        .bind(&email)
        .bind(display_name)
        .bind(&role)
        .bind(hash)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if e.as_database_error()
                .is_some_and(|d| d.code().as_deref() == Some("23505"))
            {
                AppError::code(
                    ErrorCode::Conflict,
                    "that address already has an account with this vendor",
                )
            } else {
                db(e)
            }
        })?;

        sqlx::query(
            "UPDATE vendor_invites SET accepted_at = now(), accepted_user_id = $2 WHERE id = $1",
        )
        .bind(invite_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;
        Ok((vendor_id, user_id))
    }

    async fn team(&self, vendor_id: Uuid) -> Result<Team, AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;

        let members: Vec<(
            Uuid,
            String,
            String,
            String,
            Option<time::OffsetDateTime>,
            bool,
        )> = sqlx::query_as(
            r#"SELECT id, email, display_name, role, last_login_at,
                          sso_subject IS NOT NULL AS sso_bound
                     FROM vendor_users WHERE vendor_id = $1
                    ORDER BY created_at"#,
        )
        .bind(vendor_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let invites: Vec<(Uuid, String, String, time::OffsetDateTime, bool)> = sqlx::query_as(
            r#"SELECT id, email, role, expires_at, expires_at < now()
                 FROM vendor_invites
                WHERE vendor_id = $1 AND accepted_at IS NULL
                ORDER BY created_at DESC"#,
        )
        .bind(vendor_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(Team {
            members: members
                .into_iter()
                .map(
                    |(user_id, email, display_name, role, last_login_at, sso_bound)| TeamMember {
                        user_id,
                        email,
                        display_name,
                        role,
                        last_login_at,
                        sso_bound,
                    },
                )
                .collect(),
            invites: invites
                .into_iter()
                .map(|(id, email, role, expires_at, expired)| PendingInvite {
                    id,
                    email,
                    role,
                    expires_at,
                    expired,
                })
                .collect(),
        })
    }

    async fn role_of(&self, vendor_id: Uuid, user_id: Uuid) -> Result<Role, AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;
        let role: Option<(String,)> =
            sqlx::query_as("SELECT role FROM vendor_users WHERE vendor_id = $1 AND id = $2")
                .bind(vendor_id)
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        tx.commit().await.map_err(db)?;

        role.and_then(|(r,)| Role::parse(&r))
            .ok_or_else(|| AppError::not_found("team member"))
    }

    async fn remove(&self, vendor_id: Uuid, actor: Uuid, user_id: Uuid) -> Result<(), AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;

        // The counts and the delete are in one transaction so two admins removing each other at the
        // same moment cannot both see two owners and both succeed.
        // `count(*) FOR UPDATE` is not valid Postgres, and an unlocked count would let two admins
        // removing each other both see two owners and both succeed. Lock the owner rows themselves
        // and count them in Rust.
        let owners: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM vendor_users WHERE vendor_id = $1 AND role = 'owner' FOR UPDATE",
        )
        .bind(vendor_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let target: Option<(String,)> =
            sqlx::query_as("SELECT role FROM vendor_users WHERE vendor_id = $1 AND id = $2")
                .bind(vendor_id)
                .bind(user_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;

        let Some((target_role,)) = target else {
            return Err(AppError::not_found("team member"));
        };
        let target_role =
            Role::parse(&target_role).ok_or_else(|| AppError::Internal("role".into()))?;

        let actor_role: (String,) =
            sqlx::query_as("SELECT role FROM vendor_users WHERE vendor_id = $1 AND id = $2")
                .bind(vendor_id)
                .bind(actor)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
        let actor_role =
            Role::parse(&actor_role.0).ok_or_else(|| AppError::Internal("role".into()))?;

        domain::vendor_team::may_remove(actor_role, target_role, owners.len() as i64)?;

        if actor == user_id {
            return Err(AppError::validation(
                "you cannot remove yourself; ask another owner",
            ));
        }

        sqlx::query("DELETE FROM vendor_users WHERE vendor_id = $1 AND id = $2")
            .bind(vendor_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;

        tx.commit().await.map_err(db)
    }
}
