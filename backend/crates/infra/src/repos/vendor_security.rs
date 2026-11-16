use domain::totp;
use domain::vendor_security::{
    Enrolment, ISSUER, SecurityPolicy, VendorSecurityRepo, ensure_everybody_enrolled,
};
use domain::vendor_team::{Role, VendorTeamRepo};
use kernel::{AppError, ErrorCode, TenantCtx};
use uuid::Uuid;

use crate::pg::Pg;

#[derive(Clone)]
pub struct PgVendorSecurity {
    pg: Pg,
    team: crate::repos::vendor_team::PgVendorTeam,
}

impl PgVendorSecurity {
    pub fn new(pg: Pg) -> Self {
        Self {
            team: crate::repos::vendor_team::PgVendorTeam::new(pg.clone()),
            pg,
        }
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
                request_id: "vendor-security".into(),
            })
            .await
    }

    async fn read_policy(&self, vendor_id: Uuid) -> Result<SecurityPolicy, AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;
        let row: Option<(bool, i64, i64)> = sqlx::query_as(
            r#"SELECT v.totp_required,
                      (SELECT count(*) FROM vendor_users u WHERE u.vendor_id = v.id),
                      (SELECT count(*) FROM vendor_users u
                        WHERE u.vendor_id = v.id AND u.totp_confirmed_at IS NOT NULL)
                 FROM vendors v WHERE v.id = $1"#,
        )
        .bind(vendor_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        let Some((totp_required, members, enrolled)) = row else {
            return Err(AppError::code(ErrorCode::NotFound, "no such vendor"));
        };
        Ok(SecurityPolicy {
            totp_required,
            members,
            enrolled,
        })
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

impl VendorSecurityRepo for PgVendorSecurity {
    async fn policy(&self, vendor_id: Uuid) -> Result<SecurityPolicy, AppError> {
        self.read_policy(vendor_id).await
    }

    async fn set_policy(
        &self,
        vendor_id: Uuid,
        actor: Uuid,
        totp_required: bool,
    ) -> Result<SecurityPolicy, AppError> {
        let role = self.team.role_of(vendor_id, actor).await?;
        if role != Role::Owner {
            return Err(AppError::code(
                ErrorCode::Forbidden,
                "only an owner can change how the team signs in",
            ));
        }

        // Read first, refuse on the current state: switching a factor on for somebody who has not
        // set one up locks them out, and the person pressing the toggle is not the one who suffers.
        if totp_required {
            ensure_everybody_enrolled(&self.read_policy(vendor_id).await?)?;
        }

        let mut tx = self.vendor_tx(vendor_id).await?;
        sqlx::query("UPDATE vendors SET totp_required = $2 WHERE id = $1")
            .bind(vendor_id)
            .bind(totp_required)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;

        self.read_policy(vendor_id).await
    }

    async fn enrol(&self, vendor_id: Uuid, user_id: Uuid) -> Result<Enrolment, AppError> {
        let secret = totp::new_secret();
        let mut tx = self.vendor_tx(vendor_id).await?;

        // The email is part of the `otpauth://` label, and it is the only place the person sees
        // which account the code belongs to — so it is read here rather than passed in and possibly
        // taken from the wrong session.
        let email: Option<(String,)> =
            sqlx::query_as("SELECT email FROM vendor_users WHERE id = $1 AND vendor_id = $2")
                .bind(user_id)
                .bind(vendor_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;

        let Some((email,)) = email else {
            return Err(AppError::code(ErrorCode::NotFound, "no such member"));
        };

        // Re-enrolling clears the confirmation: a new secret nobody has proved yet must not count as
        // a factor, or a member could swap in a secret and leave the account protected by nothing
        // they can use.
        let updated = sqlx::query(
            "UPDATE vendor_users SET totp_secret = $2, totp_confirmed_at = NULL
              WHERE id = $1 AND vendor_id = $3",
        )
        .bind(user_id)
        .bind(&secret)
        .bind(vendor_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        tx.commit().await.map_err(db)?;

        if updated == 0 {
            return Err(AppError::code(ErrorCode::NotFound, "no such member"));
        }

        Ok(Enrolment {
            otpauth_uri: totp::enrolment_uri(ISSUER, &email, &secret),
            secret,
        })
    }

    async fn confirm(
        &self,
        vendor_id: Uuid,
        user_id: Uuid,
        code: &str,
    ) -> Result<SecurityPolicy, AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;
        let stored: Option<(Option<String>,)> =
            sqlx::query_as("SELECT totp_secret FROM vendor_users WHERE id = $1 AND vendor_id = $2")
                .bind(user_id)
                .bind(vendor_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;

        let Some((Some(secret),)) = stored else {
            return Err(AppError::code(
                ErrorCode::Conflict,
                "start an enrolment first: there is no secret to confirm",
            ));
        };

        if !totp::verify(
            &secret,
            code,
            time::OffsetDateTime::now_utc().unix_timestamp(),
        ) {
            return Err(AppError::code(
                ErrorCode::Unauthorized,
                "that code does not match; check the clock on your phone",
            ));
        }

        sqlx::query(
            "UPDATE vendor_users SET totp_confirmed_at = now() WHERE id = $1 AND vendor_id = $2",
        )
        .bind(user_id)
        .bind(vendor_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        self.read_policy(vendor_id).await
    }

    async fn disable(&self, vendor_id: Uuid, user_id: Uuid) -> Result<SecurityPolicy, AppError> {
        let policy = self.read_policy(vendor_id).await?;
        if policy.totp_required {
            return Err(AppError::code(
                ErrorCode::Conflict,
                "two-factor is required for this vendor; an owner has to turn it off first",
            ));
        }

        let mut tx = self.vendor_tx(vendor_id).await?;
        sqlx::query(
            "UPDATE vendor_users SET totp_secret = NULL, totp_confirmed_at = NULL
              WHERE id = $1 AND vendor_id = $2",
        )
        .bind(user_id)
        .bind(vendor_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        self.read_policy(vendor_id).await
    }
}
