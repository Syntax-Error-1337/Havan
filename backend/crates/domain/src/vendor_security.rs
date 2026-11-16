//! Two-factor policy for a vendor's people.
//!
//! The rule that makes this safe is the same one that makes SSO safe: a policy may only be switched
//! on when nobody is left unable to sign in. Turning TOTP on while a member has not enrolled locks
//! that person out at the next login, and the person who did it is not the one who suffers.

use std::future::Future;

use kernel::{AppError, ErrorCode};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct SecurityPolicy {
    pub totp_required: bool,
    pub members: i64,
    /// Members who have proved they can generate a code.
    pub enrolled: i64,
}

/// A secret shown once, and the URI that turns it into a QR code.
#[derive(Debug, Clone, Serialize)]
pub struct Enrolment {
    pub secret: String,
    pub otpauth_uri: String,
}

pub const ISSUER: &str = "Haven";

/// Refuse to require a factor from somebody who has not set one up.
pub fn ensure_everybody_enrolled(policy: &SecurityPolicy) -> Result<(), AppError> {
    // Only when it is being *required*. Turning the policy off must never be blocked by somebody
    // who has not enrolled — that is the state you are trying to escape.
    if policy.totp_required && policy.enrolled < policy.members {
        let mut err = AppError::code(
            ErrorCode::Conflict,
            "every member must enrol before two-factor can be required",
        );
        if let AppError::Coded { details, .. } = &mut err {
            details.insert("enrolled".into(), policy.enrolled.to_string());
            details.insert("members".into(), policy.members.to_string());
        }
        return Err(err);
    }
    Ok(())
}

pub trait VendorSecurityRepo {
    fn policy(
        &self,
        vendor_id: Uuid,
    ) -> impl Future<Output = Result<SecurityPolicy, AppError>> + Send;

    /// Owner-only. Refused while any member has not enrolled.
    fn set_policy(
        &self,
        vendor_id: Uuid,
        actor: Uuid,
        totp_required: bool,
    ) -> impl Future<Output = Result<SecurityPolicy, AppError>> + Send;

    /// Start or restart enrolment for one member. The secret is returned once and stored; the
    /// member is not enrolled until they prove they can read a code from it.
    fn enrol(
        &self,
        vendor_id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Enrolment, AppError>> + Send;

    fn confirm(
        &self,
        vendor_id: Uuid,
        user_id: Uuid,
        code: &str,
    ) -> impl Future<Output = Result<SecurityPolicy, AppError>> + Send;

    /// Refused while the policy requires two-factor: someone who can switch it off for themselves
    /// is not a second factor.
    fn disable(
        &self,
        vendor_id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<SecurityPolicy, AppError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_policy_is_only_enforceable_once_everybody_enrolled() {
        let partial = SecurityPolicy {
            totp_required: true,
            members: 2,
            enrolled: 1,
        };
        assert_eq!(
            ensure_everybody_enrolled(&partial).unwrap_err().code_kind(),
            ErrorCode::Conflict
        );

        let ready = SecurityPolicy {
            totp_required: true,
            members: 2,
            enrolled: 2,
        };
        assert!(ensure_everybody_enrolled(&ready).is_ok());

        // Off is always allowed, however many people have enrolled.
        let off = SecurityPolicy {
            totp_required: false,
            members: 3,
            enrolled: 1,
        };
        assert!(ensure_everybody_enrolled(&off).is_ok());
    }
}
