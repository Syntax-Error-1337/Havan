use domain::sso::{SsoConfig, SsoRepo, may_configure_sso};
use domain::vendor_team::{Role, VendorTeamRepo};
use kernel::{AppError, TenantCtx};
use uuid::Uuid;

use crate::pg::Pg;

#[derive(Clone)]
pub struct PgSso {
    pg: Pg,
    team: crate::repos::vendor_team::PgVendorTeam,
}

impl PgSso {
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
                request_id: "sso".into(),
            })
            .await
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

impl SsoRepo for PgSso {
    async fn config(&self, vendor_id: Uuid) -> Result<SsoConfig, AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;
        let row: Option<(bool, Option<String>, i64, i64)> = sqlx::query_as(
            r#"SELECT v.sso_required, v.sso_domain,
                      (SELECT count(*) FROM vendor_users u
                        WHERE u.vendor_id = v.id AND u.sso_subject IS NOT NULL),
                      (SELECT count(*) FROM vendor_users u WHERE u.vendor_id = v.id)
                 FROM vendors v WHERE v.id = $1"#,
        )
        .bind(vendor_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        let Some((required, domain, bound, total)) = row else {
            return Err(AppError::not_found("vendor"));
        };

        Ok(SsoConfig {
            required,
            domain,
            bound_members: bound,
            total_members: total,
            // Until an assertion can be validated, nothing is enforced — and the API says so rather
            // than letting a vendor believe they are protected.
            enforced: false,
        })
    }

    async fn configure(
        &self,
        vendor_id: Uuid,
        actor: Uuid,
        required: bool,
        domain: Option<&str>,
    ) -> Result<SsoConfig, AppError> {
        let role = self.team.role_of(vendor_id, actor).await?;
        may_configure_sso(role)?;

        domain::sso::validate_domain(domain.unwrap_or(""))?;

        // Turning it on while somebody has no identity bound is the one way this feature can hurt a
        // team: the moment enforcement lands, that person cannot sign in at all. Refusing here means
        // the mistake is caught by the owner pressing the toggle, not by a colleague at 9am.
        if required {
            let counts = self.config(vendor_id).await?;
            if counts.bound_members < counts.total_members {
                let mut err = AppError::code(
                    kernel::ErrorCode::Conflict,
                    "every member needs an identity bound before SSO can be required",
                );
                if let AppError::Coded { details, .. } = &mut err {
                    details.insert("bound".into(), counts.bound_members.to_string());
                    details.insert("total".into(), counts.total_members.to_string());
                }
                return Err(err);
            }
        }

        let mut tx = self.vendor_tx(vendor_id).await?;
        sqlx::query("UPDATE vendors SET sso_required = $2, sso_domain = $3 WHERE id = $1")
            .bind(vendor_id)
            .bind(required)
            .bind(domain.map(str::trim).filter(|d| !d.is_empty()))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;

        self.config(vendor_id).await
    }

    async fn bind_subject(
        &self,
        vendor_id: Uuid,
        user_id: Uuid,
        subject: &str,
    ) -> Result<(), AppError> {
        let subject = subject.trim();

        // An empty subject is how a binding is removed — the screen offers "clear" beside "bind",
        // and a flow with no way back is a flow that traps somebody's identifier in a table. It is
        // refused while SSO is required, for the same reason the TOTP policy cannot be satisfied
        // by locking its own author out: the person who enabled enforcement has to switch it off
        // before anybody can leave the identity they signed in with.
        if subject.is_empty() {
            if self.config(vendor_id).await?.required {
                return Err(AppError::code(
                    kernel::ErrorCode::Conflict,
                    "SSO is required for this vendor; an owner has to turn it off before unbinding an identity",
                ));
            }
            let mut tx = self.vendor_tx(vendor_id).await?;
            sqlx::query(
                "UPDATE vendor_users SET sso_subject = NULL WHERE vendor_id = $1 AND id = $2",
            )
            .bind(vendor_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            tx.commit().await.map_err(db)?;
            return Ok(());
        }

        if subject.len() < 3 || subject.len() > 256 {
            return Err(AppError::validation(
                "an identity-provider subject is between 3 and 256 characters",
            ));
        }

        let mut tx = self.vendor_tx(vendor_id).await?;
        let n = sqlx::query(
            "UPDATE vendor_users SET sso_subject = $3 WHERE vendor_id = $1 AND id = $2",
        )
        .bind(vendor_id)
        .bind(user_id)
        .bind(subject)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if e.as_database_error()
                .is_some_and(|d| d.code().as_deref() == Some("23505"))
            {
                AppError::code(
                    kernel::ErrorCode::Conflict,
                    "that identity is already bound to another member",
                )
            } else {
                db(e)
            }
        })?
        .rows_affected();
        tx.commit().await.map_err(db)?;

        if n == 0 {
            return Err(AppError::not_found("team member"));
        }
        Ok(())
    }
}

/// Re-exported so handlers can name the role type without importing two modules.
pub type TeamRole = Role;
