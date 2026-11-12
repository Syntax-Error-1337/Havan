//! What a vendor can see about their own fleet.
//!
//! Read-only and vendor-scoped: the tenant boundary is the `vendor_id` on every row,
//! enforced by row-level security, so these queries cannot accidentally span two
//! vendors even if a `WHERE` is forgotten later.
//!
//! Deliberately *not* here: which customer owns which device, their home's name, or
//! anything else that identifies a person. A vendor sells hardware; they get to know
//! how the hardware is doing, not who bought it.

use std::future::Future;
use std::sync::Arc;

use kernel::AppError;
use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

/// One device in the fleet list.
#[derive(Debug, Clone, Serialize)]
pub struct FleetDevice {
    pub id: Uuid,
    pub product_key: String,
    pub product_name: String,
    pub name: String,
    pub serial: String,
    pub state: String,
    /// Claimed by a customer. Unbound devices are stock in the field.
    pub bound: bool,
    pub online: bool,
    pub fw_version: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_seen_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

/// A page of the fleet. `cursor` is the last id of the previous page, which is
/// `(created_at, id)` ordered — a keyset, not an offset, so a device claiming during
/// the read does not shift rows between pages.
#[derive(Debug, Clone, Serialize)]
pub struct FleetPage {
    pub devices: Vec<FleetDevice>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct FleetFilter {
    pub product_key: Option<String>,
    /// `online`, `offline`, `bound`, `unbound`.
    pub state: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
}

impl FleetFilter {
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(50).clamp(1, 200)
    }
}

/// Aggregate numbers for the analytics screen. Every one of these is a count over
/// rows the vendor can already see — no sampling, no estimates dressed as facts.
#[derive(Debug, Clone, Serialize, Default)]
pub struct FleetStats {
    pub devices: Counts,
    pub new_devices: NewDevices,
    pub by_product: Vec<ProductCount>,
    pub firmware: Vec<FirmwareCount>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Counts {
    pub total: i64,
    pub online: i64,
    pub bound: i64,
    pub unbound: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct NewDevices {
    pub last_7d: i64,
    pub last_30d: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductCount {
    pub product_key: String,
    pub name: String,
    pub total: i64,
    pub online: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FirmwareCount {
    /// `None` for devices that have never reported a version.
    pub version: Option<String>,
    pub devices: i64,
}

pub trait FleetRepo {
    fn devices(
        &self,
        vendor_id: Uuid,
        filter: &FleetFilter,
    ) -> impl Future<Output = Result<Vec<FleetDevice>, AppError>> + Send;

    fn stats(&self, vendor_id: Uuid) -> impl Future<Output = Result<FleetStats, AppError>> + Send;
}

pub struct FleetService<R> {
    repo: Arc<R>,
}

impl<R: FleetRepo + Send + Sync> FleetService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    /// A page of the fleet. `next_cursor` is present only when there might be more:
    /// a page shorter than the limit is the end, and saying otherwise makes a client
    /// ask again for nothing.
    pub async fn page(&self, vendor_id: Uuid, filter: FleetFilter) -> Result<FleetPage, AppError> {
        let limit = filter.limit();
        let mut devices = self.repo.devices(vendor_id, &filter).await?;

        let next_cursor = if i64::try_from(devices.len()).unwrap_or(0) > limit {
            devices.truncate(usize::try_from(limit).unwrap_or(50));
            devices.last().map(|d| d.id.to_string())
        } else {
            None
        };

        Ok(FleetPage {
            devices,
            next_cursor,
        })
    }

    pub async fn stats(&self, vendor_id: Uuid) -> Result<FleetStats, AppError> {
        self.repo.stats(vendor_id).await
    }
}

/// The filters a caller may use, checked here rather than in SQL so an unknown value
/// is a `422` with a sentence instead of an empty page that looks like "no devices".
pub fn validate(filter: &FleetFilter) -> Result<(), AppError> {
    match filter.state.as_deref() {
        None | Some("online" | "offline" | "bound" | "unbound") => Ok(()),
        Some(other) => Err(AppError::validation(format!(
            "unknown state filter {other}: use online, offline, bound or unbound"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        devices: Vec<FleetDevice>,
    }

    impl FleetRepo for Fake {
        async fn devices(
            &self,
            _vendor_id: Uuid,
            _filter: &FleetFilter,
        ) -> Result<Vec<FleetDevice>, AppError> {
            Ok(self.devices.clone())
        }

        async fn stats(&self, _vendor_id: Uuid) -> Result<FleetStats, AppError> {
            Ok(FleetStats::default())
        }
    }

    fn device(n: u128) -> FleetDevice {
        FleetDevice {
            id: Uuid::from_u128(n),
            product_key: "k".into(),
            product_name: "K".into(),
            name: format!("d{n}"),
            serial: format!("S{n}"),
            state: "claimed".into(),
            bound: true,
            online: false,
            fw_version: None,
            last_seen_at: None,
            created_at: kernel::now(),
        }
    }

    #[tokio::test]
    async fn a_full_page_points_at_the_next_one() {
        let service = FleetService::new(Arc::new(Fake {
            devices: (1..=3).map(device).collect(),
        }));
        // The repo returns `limit + 1` when there is more; the service trims and
        // reports the cursor.
        let page = service
            .page(
                Uuid::from_u128(9),
                FleetFilter {
                    limit: Some(2),
                    ..FleetFilter::default()
                },
            )
            .await
            .expect("page");

        assert_eq!(page.devices.len(), 2);
        assert_eq!(page.next_cursor, Some(Uuid::from_u128(2).to_string()));
    }

    #[tokio::test]
    async fn a_short_page_is_the_end() {
        let service = FleetService::new(Arc::new(Fake {
            devices: vec![device(1)],
        }));
        let page = service
            .page(
                Uuid::from_u128(9),
                FleetFilter {
                    limit: Some(2),
                    ..FleetFilter::default()
                },
            )
            .await
            .expect("page");
        assert_eq!(page.next_cursor, None, "a short page must not promise more");
    }

    #[test]
    fn the_limit_is_bounded() {
        assert_eq!(FleetFilter::default().limit(), 50);
        assert_eq!(
            FleetFilter {
                limit: Some(5_000),
                ..FleetFilter::default()
            }
            .limit(),
            200
        );
    }

    #[test]
    fn an_unknown_state_filter_is_a_validation_error() {
        assert!(validate(&FleetFilter::default()).is_ok());
        assert!(
            validate(&FleetFilter {
                state: Some("online".into()),
                ..FleetFilter::default()
            })
            .is_ok()
        );
        assert!(
            validate(&FleetFilter {
                state: Some("sleepy".into()),
                ..FleetFilter::default()
            })
            .is_err()
        );
    }
}
