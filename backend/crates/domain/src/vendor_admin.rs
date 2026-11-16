//! The operator's view of their tenants: who sells on this platform, and whether they may.
//!
//! A vendor row is not a customer's data — it is the operator's own book of business. Everything
//! here runs on the ops door and never appears in a vendor response.

use std::future::Future;

use kernel::AppError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The lifecycle a vendor can be in. `deleted` is in the schema and deliberately not reachable from
/// the console: deleting a tenant with devices in people's homes is a decision made with a database
/// client and a second person, not a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VendorStatus {
    Active,
    Suspended,
}

impl VendorStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
        }
    }

    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "active" => Ok(Self::Active),
            "suspended" => Ok(Self::Suspended),
            "deleted" => Err(AppError::validation(
                "a deleted vendor is not managed from here",
            )),
            other => Err(AppError::validation(format!(
                "unknown vendor status {other:?}"
            ))),
        }
    }
}

/// One row of the vendor list, with the numbers an operator actually asks about.
#[derive(Debug, Clone, Serialize)]
pub struct VendorRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub status: String,
    /// Null means no ceiling agreed; the licence ledger is what limits activations.
    pub quota_devices: Option<i32>,
    pub totp_required: bool,
    pub products: i64,
    pub devices: i64,
    /// Licences authorised that have not expired.
    pub granted: i64,
    /// Devices activated, all time.
    pub used: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewVendor {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub quota_devices: Option<i32>,
}

impl NewVendor {
    /// A slug is a URL and a support handle, so it is constrained here rather than discovered later
    /// by a customer reading it in a link.
    pub fn validate(&self) -> Result<(), AppError> {
        let slug = self.slug.trim();
        let valid = (3..=40).contains(&slug.len())
            && slug
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            && !slug.starts_with('-')
            && !slug.ends_with('-');

        if !valid {
            return Err(AppError::validation(
                "slug must be 3-40 characters of a-z, 0-9 and -, not starting or ending with -",
            ));
        }

        let name = self.name.trim();
        if name.is_empty() || name.len() > 80 {
            return Err(AppError::validation(
                "name must be between 1 and 80 characters",
            ));
        }

        if let Some(quota) = self.quota_devices {
            if !(1..=1_000_000).contains(&quota) {
                return Err(AppError::validation(
                    "quota_devices must be between 1 and 1000000",
                ));
            }
        }

        Ok(())
    }

    pub fn slug(&self) -> String {
        self.slug.trim().to_ascii_lowercase()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct VendorPatch {
    #[serde(default)]
    pub status: Option<String>,
    /// `Some(None)` clears the ceiling; `None` leaves it alone.
    #[serde(default, deserialize_with = "double_option")]
    pub quota_devices: Option<Option<i32>>,
    #[serde(default)]
    pub name: Option<String>,
}

/// Distinguishes "field absent" from "field set to null" without a second DTO.
fn double_option<'de, D>(de: D) -> Result<Option<Option<i32>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<i32>::deserialize(de).map(Some)
}

impl VendorPatch {
    pub fn status(&self) -> Result<Option<VendorStatus>, AppError> {
        self.status
            .as_deref()
            .map(VendorStatus::parse)
            .transpose()
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if let Some(Some(quota)) = self.quota_devices {
            if !(1..=1_000_000).contains(&quota) {
                return Err(AppError::validation(
                    "quota_devices must be between 1 and 1000000",
                ));
            }
        }
        if let Some(name) = &self.name {
            if name.trim().is_empty() || name.len() > 80 {
                return Err(AppError::validation(
                    "name must be between 1 and 80 characters",
                ));
            }
        }
        Ok(())
    }
}

pub trait VendorAdminRepo {
    fn list(&self, limit: i64) -> impl Future<Output = Result<Vec<VendorRow>, AppError>> + Send;

    fn one(&self, id: Uuid) -> impl Future<Output = Result<VendorRow, AppError>> + Send;

    fn create(&self, new: &NewVendor) -> impl Future<Output = Result<VendorRow, AppError>> + Send;

    fn patch(
        &self,
        id: Uuid,
        patch: &VendorPatch,
    ) -> impl Future<Output = Result<VendorRow, AppError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vendor(slug: &str, name: &str) -> NewVendor {
        NewVendor {
            slug: slug.into(),
            name: name.into(),
            quota_devices: None,
        }
    }

    #[test]
    fn slugs_are_the_shape_a_url_needs() {
        assert!(vendor("acme-home", "Acme").validate().is_ok());
        for bad in ["Acme", "ac me", "-acme", "acme-", "ab", "a".repeat(41).as_str()] {
            assert!(
                vendor(bad, "Acme").validate().is_err(),
                "{bad:?} was accepted as a slug"
            );
        }
    }

    #[test]
    fn a_name_cannot_be_empty() {
        assert!(vendor("acme", "   ").validate().is_err());
        assert!(vendor("acme", &"x".repeat(81)).validate().is_err());
    }

    #[test]
    fn the_slug_is_lowercased_rather_than_refused() {
        // A vendor typing "Acme" in a form is not making a mistake worth a 422.
        let mut new = vendor("acme", "Acme");
        new.slug = "ACME".into();
        assert_eq!(new.slug(), "acme");
    }

    #[test]
    fn deleted_is_not_reachable_from_the_console() {
        assert_eq!(VendorStatus::parse("active").unwrap(), VendorStatus::Active);
        assert_eq!(
            VendorStatus::parse("suspended").unwrap(),
            VendorStatus::Suspended
        );
        assert!(VendorStatus::parse("deleted").is_err());
        assert!(VendorStatus::parse("paused").is_err());
    }
}
