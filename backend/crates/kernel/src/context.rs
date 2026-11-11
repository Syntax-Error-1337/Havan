//! Who is acting, and for which tenant.
//!
//! Every repository call that touches tenant data takes a `&TenantCtx`, and
//! every tenant-scoped query runs inside a transaction that has already set
//! `haven.vendor_id`. A missing context is a compile error, not a slow leak.

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    /// A person using the app or the portal.
    User {
        user_id: Uuid,
        /// Present when the request came through a vendor session.
        vendor_id: Option<Uuid>,
    },
    /// A vendor's server, via an API key.
    ApiKey { vendor_id: Uuid, key_id: Uuid },
    /// A device, authenticated by its certificate.
    Device { device_id: Uuid },
    /// A background job. The only actor allowed to cross tenants.
    System { job: &'static str },
    /// An unauthenticated caller on a route that is deliberately public
    /// (claim, health). Carries an IP for rate limiting.
    Anonymous { ip: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantCtx {
    pub vendor_id: Option<Uuid>,
    pub actor: Actor,
    /// Correlation id, echoed in `X-Request-Id` and every log line.
    pub request_id: String,
}

impl TenantCtx {
    pub fn user(user_id: Uuid, request_id: impl Into<String>) -> Self {
        Self {
            vendor_id: None,
            actor: Actor::User {
                user_id,
                vendor_id: None,
            },
            request_id: request_id.into(),
        }
    }

    pub fn vendor_user(vendor_id: Uuid, user_id: Uuid, request_id: impl Into<String>) -> Self {
        Self {
            vendor_id: Some(vendor_id),
            actor: Actor::User {
                user_id,
                vendor_id: Some(vendor_id),
            },
            request_id: request_id.into(),
        }
    }

    pub fn system(job: &'static str) -> Self {
        Self {
            vendor_id: None,
            actor: Actor::System { job },
            request_id: format!("sys-{job}"),
        }
    }

    pub fn device(device_id: Uuid, vendor_id: Uuid) -> Self {
        Self {
            vendor_id: Some(vendor_id),
            actor: Actor::Device { device_id },
            request_id: format!("dev-{device_id}"),
        }
    }

    pub fn is_system(&self) -> bool {
        matches!(self.actor, Actor::System { .. })
    }

    pub fn user_id(&self) -> Option<Uuid> {
        match self.actor {
            Actor::User { user_id, .. } => Some(user_id),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_system_may_cross_tenants() {
        let sys = TenantCtx::system("retention");
        assert!(sys.is_system());
        assert!(sys.vendor_id.is_none());

        let u = TenantCtx::user(Uuid::nil(), "req-1");
        assert!(!u.is_system());
        assert_eq!(u.user_id(), Some(Uuid::nil()));
    }
}
