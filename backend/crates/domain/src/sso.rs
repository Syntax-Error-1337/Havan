//! SSO configuration: what is stored now, and what is deliberately not enforced yet.
//!
//! A vendor with an IdP needs three things from the platform: a place to record "we require SSO", a
//! place to record which IdP identity belongs to which account, and a rule that binding one is
//! explicit rather than inferred from an email address that anyone can type.
//!
//! What is missing is the part that cannot be faked: validating a SAML/OIDC assertion against a real
//! provider. Until that exists, `sso_required` is a recorded intent that **does not** disable
//! password login. Enforcing it now would lock a vendor out of its own portal the moment they asked
//! for it, with no way back in — which is a worse outcome than the gap it closes.

use kernel::AppError;
use serde::Serialize;
use uuid::Uuid;

use crate::vendor_team::Role;

/// Owner-only: turning SSO on changes how everyone in the company logs in.
pub fn may_configure_sso(actor: Role) -> Result<(), AppError> {
    if actor == Role::Owner {
        Ok(())
    } else {
        Err(AppError::forbidden(
            "only an owner can change how the company signs in",
        ))
    }
}

/// A domain is a routing hint, not a security boundary. Refusing a malformed one keeps the portal
/// from storing `@` and pretending it means something.
pub fn validate_domain(domain: &str) -> Result<(), AppError> {
    let trimmed = domain.trim();
    if trimmed.is_empty() {
        return Ok(()); // clearing it is allowed
    }
    if !trimmed.contains('.') || trimmed.starts_with('@') || trimmed.contains(char::is_whitespace) {
        return Err(AppError::validation(
            "a domain looks like example.com, without an @ or a path",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct SsoConfig {
    pub required: bool,
    pub domain: Option<String>,
    /// How many members already have an identity bound. `required` with unbound members is the
    /// configuration that would lock people out once enforcement lands, so it is reported up front.
    pub bound_members: i64,
    pub total_members: i64,
    pub enforced: bool,
}

pub trait SsoRepo {
    fn configure(
        &self,
        vendor_id: Uuid,
        actor: Uuid,
        required: bool,
        domain: Option<&str>,
    ) -> impl std::future::Future<Output = Result<SsoConfig, AppError>> + Send;

    fn bind_subject(
        &self,
        vendor_id: Uuid,
        user_id: Uuid,
        subject: &str,
    ) -> impl std::future::Future<Output = Result<(), AppError>> + Send;

    fn config(
        &self,
        vendor_id: Uuid,
    ) -> impl std::future::Future<Output = Result<SsoConfig, AppError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_owner_may_configure_sso() {
        assert!(may_configure_sso(Role::Owner).is_ok());
        for role in [Role::Admin, Role::Developer, Role::Viewer] {
            assert!(may_configure_sso(role).is_err());
        }
    }

    #[test]
    fn domains_are_checked_but_clearing_is_allowed() {
        assert!(validate_domain("acme.example").is_ok());
        assert!(validate_domain("").is_ok());
        assert!(validate_domain("acme").is_err());
        assert!(validate_domain("@acme.example").is_err());
        assert!(validate_domain("acme example.com").is_err());
    }
}
