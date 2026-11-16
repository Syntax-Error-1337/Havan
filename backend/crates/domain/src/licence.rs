//! Device licences: what a vendor bought, and what it has activated.
//!
//! The platform charges per device that goes live in our app, so the licence is not a
//! subscription and not a seat — it is a row in `licence_activations`, written by the same
//! transaction that takes the serial out of the manufacturing manifest.
//!
//! Two things this module deliberately does *not* do:
//!
//! - it does not store a balance. The balance is `grants - activations`, derived on every
//!   read, because a stored counter can drift from the rows it summarises and the first
//!   drift bills someone for a device that does not exist;
//! - it does not decide the price. A grant carries the price the operator sold at, and an
//!   activation copies it, so changing the price tomorrow cannot rewrite yesterday's bill.

use std::future::Future;

use kernel::AppError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A ceiling on a single grant, so a typo cannot authorise a million activations.
pub const MAX_GRANT: i64 = 1_000_000;

#[derive(Debug, Clone, Serialize)]
pub struct LicenceSummary {
    /// Activations the operator has authorised and that have not expired.
    pub granted: i64,
    /// Devices activated, all time.
    pub used: i64,
    /// `granted - used`. Negative means a grant was revoked after devices were live,
    /// which is worth seeing rather than clamping away.
    pub remaining: i64,
    /// Activations in the current calendar month — what this month's invoice is built from.
    pub activated_this_period: i64,
    pub granted_value_cents: i64,
    pub used_value_cents: i64,
    /// The newest grant's unit price, for the "buy more" screen.
    pub unit_price_cents: i32,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActivationRow {
    pub serial: String,
    pub product_key: String,
    pub product_name: String,
    #[serde(with = "time::serde::rfc3339")]
    pub activated_at: time::OffsetDateTime,
    pub unit_price_cents: i32,
}

/// What an operator asks for when it grants activations to a vendor.
#[derive(Debug, Clone, Deserialize)]
pub struct NewGrant {
    pub vendor_id: Uuid,
    pub quantity: i64,
    pub unit_price_cents: i32,
    #[serde(default)]
    pub currency: Option<String>,
    /// Days from now. Absent means the grant never expires.
    #[serde(default)]
    pub expires_in_days: Option<i64>,
    #[serde(default)]
    pub note: Option<String>,
}

impl NewGrant {
    /// Reject nonsense before it reaches the database, with a message an operator can act on.
    pub fn validate(&self) -> Result<(), AppError> {
        if self.quantity < 1 || self.quantity > MAX_GRANT {
            return Err(AppError::validation(format!(
                "quantity must be between 1 and {MAX_GRANT}"
            )));
        }
        if self.unit_price_cents < 0 {
            return Err(AppError::validation("unit_price_cents cannot be negative"));
        }
        let currency = self.currency.as_deref().unwrap_or("USD");
        if currency.len() != 3 || !currency.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(AppError::validation(
                "currency must be a three-letter code, e.g. USD",
            ));
        }
        if self.expires_in_days.is_some_and(|d| d < 1) {
            return Err(AppError::validation("expires_in_days must be positive"));
        }
        Ok(())
    }

    pub fn currency(&self) -> String {
        self.currency
            .as_deref()
            .unwrap_or("USD")
            .to_ascii_uppercase()
    }
}

pub trait LicenceRepo {
    /// The vendor's own balance, for the portal.
    fn summary(
        &self,
        vendor_id: Uuid,
    ) -> impl Future<Output = Result<LicenceSummary, AppError>> + Send;

    /// Recent activations, newest first. The vendor sees their own devices only, by
    /// serial and product — never who activated them.
    fn activations(
        &self,
        vendor_id: Uuid,
        limit: i64,
    ) -> impl Future<Output = Result<Vec<ActivationRow>, AppError>> + Send;

    /// Operator-only: authorise more activations.
    fn grant(&self, grant: &NewGrant) -> impl Future<Output = Result<Uuid, AppError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(quantity: i64, price: i32) -> NewGrant {
        NewGrant {
            vendor_id: Uuid::nil(),
            quantity,
            unit_price_cents: price,
            currency: None,
            expires_in_days: None,
            note: None,
        }
    }

    #[test]
    fn accepts_a_normal_grant() {
        assert!(grant(1_000, 249).validate().is_ok());
    }

    #[test]
    fn refuses_quantities_that_are_not_sales() {
        assert!(grant(0, 249).validate().is_err());
        assert!(grant(-5, 249).validate().is_err());
        assert!(grant(MAX_GRANT + 1, 249).validate().is_err());
    }

    #[test]
    fn refuses_a_negative_price_but_allows_free() {
        assert!(grant(10, -1).validate().is_err());
        assert!(grant(10, 0).validate().is_ok());
    }

    #[test]
    fn refuses_a_currency_that_is_not_a_currency() {
        let mut g = grant(10, 100);
        g.currency = Some("dollars".into());
        assert!(g.validate().is_err());
        g.currency = Some("eur".into());
        assert_eq!(g.currency(), "EUR");
    }

    #[test]
    fn expiry_must_be_in_the_future() {
        let mut g = grant(10, 100);
        g.expires_in_days = Some(0);
        assert!(g.validate().is_err());
        g.expires_in_days = Some(30);
        assert!(g.validate().is_ok());
    }
}
