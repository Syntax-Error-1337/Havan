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
    /// What a support engineer pastes in: part of a serial, or part of a device name.
    ///
    /// `Option<String>` rather than a bool, because the search box is empty most of the time and
    /// `"%%"` is not a filter.
    pub q: Option<String>,
    /// `online`, `offline`, `bound`, `unbound`.
    pub state: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<i64>,
    /// What the table is ordered by. Absent means "newest first", which is what the page did before it could sort.
    pub sort: Option<String>,
    /// `asc` or `desc`. Absent means the direction that field is usually read in.
    pub dir: Option<String>,
}

/// The columns a fleet table can be ordered by.
///
/// A closed set, because the value ends up in an `ORDER BY`: an enum mapped to a literal is the difference between a
/// sort and an injection, and there is no reason to accept a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FleetSort {
    /// When the serial was manufactured into the platform — the only "newest" a vendor has.
    #[default]
    Created,
    Serial,
    Name,
    LastSeen,
}

impl FleetSort {
    pub fn parse(raw: &str) -> Result<Self, AppError> {
        match raw {
            "created" => Ok(Self::Created),
            "serial" => Ok(Self::Serial),
            "name" => Ok(Self::Name),
            "last_seen" => Ok(Self::LastSeen),
            other => Err(AppError::validation(format!(
                "unknown sort {other}: use created, serial, name or last_seen"
            ))),
        }
    }

    /// The expression the keyset compares and the page orders by, for one table alias.
    ///
    /// A device that has never reported has no `last_seen_at`; it sorts as older than any device that has, rather than
    /// sinking the comparison to NULL. Without the `COALESCE` the whole tuple comparison goes NULL and every never-seen
    /// device disappears from page two onwards.
    ///
    /// Takes the alias because the keyset needs the same expression twice — once for the row, once for the cursor row it
    /// is compared against — and the two are different aliases of the same table.
    pub fn expression(self, alias: &str) -> String {
        match self {
            Self::Created => format!("{alias}.created_at"),
            Self::Serial => format!("{alias}.serial"),
            Self::Name => format!("{alias}.name"),
            Self::LastSeen => format!("COALESCE({alias}.last_seen_at, 'epoch'::timestamptz)"),
        }
    }
}

impl FleetFilter {
    /// The order the caller asked for, validated.
    pub fn sort(&self) -> Result<FleetSort, AppError> {
        match self.sort.as_deref() {
            None | Some("") => Ok(FleetSort::Created),
            Some(raw) => FleetSort::parse(raw),
        }
    }

    /// Descending unless the caller said otherwise.
    ///
    /// The default per field is the direction it is usually read in: serials and names ascend (A–Z), timestamps descend
    /// (newest first). Asking for `serial` and getting Z–A would be a surprise dressed as consistency.
    pub fn descending(&self) -> Result<bool, AppError> {
        match self.dir.as_deref() {
            None | Some("") => Ok(!matches!(self.sort()?, FleetSort::Serial | FleetSort::Name)),
            Some("asc") => Ok(false),
            Some("desc") => Ok(true),
            Some(other) => Err(AppError::validation(format!(
                "direction must be asc or desc, not {other}"
            ))),
        }
    }

    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(50).clamp(1, 200)
    }

    /// The search term, or `None` when there is nothing to search for.
    ///
    /// Trimmed, capped at 64 characters (a serial is shorter, and an unbounded pattern is a way to make
    /// the database do work for free), and with `%` and `_` escaped so that typing one does not silently
    /// turn into a wildcard.
    pub fn query(&self) -> Option<String> {
        let raw = self.q.as_deref()?.trim();
        if raw.is_empty() {
            return None;
        }
        let capped: String = raw.chars().take(64).collect();
        Some(
            capped
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_"),
        )
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
        None | Some("online" | "offline" | "bound" | "unbound") => {}
        Some(other) => {
            return Err(AppError::validation(format!(
                "unknown state filter {other}: use online, offline, bound or unbound"
            )));
        }
    }
    // Both are checked here as well as at the point of use: a bad `sort` should be a 422 that says which value is
    // wrong, not a 500 when the query is built.
    filter.sort()?;
    filter.descending()?;
    Ok(())
}

#[cfg(test)]
mod sort_tests {
    use super::*;

    #[test]
    fn a_sort_column_is_a_closed_set_and_says_which_value_is_wrong() {
        assert_eq!(
            FleetFilter::default().sort().expect("default"),
            FleetSort::Created
        );
        assert_eq!(
            FleetFilter {
                sort: Some("serial".into()),
                ..Default::default()
            }
            .sort()
            .expect("serial"),
            FleetSort::Serial
        );

        // The value ends up in an ORDER BY, so an unknown one is refused rather than passed through.
        let err = FleetFilter {
            sort: Some("d.created_at; DROP TABLE devices".into()),
            ..Default::default()
        }
        .sort()
        .expect_err("must refuse");
        assert!(err.public_message().contains("unknown sort"));
    }

    #[test]
    fn the_default_direction_is_the_one_the_field_is_read_in() {
        let desc = |sort: &str, dir: Option<&str>| {
            FleetFilter {
                sort: Some(sort.into()),
                dir: dir.map(str::to_string),
                ..Default::default()
            }
            .descending()
            .expect("valid")
        };

        // Timestamps newest first, text A-Z: asking for `serial` and getting Z-A is a surprise dressed as consistency.
        assert!(desc("created", None));
        assert!(desc("last_seen", None));
        assert!(!desc("serial", None));
        assert!(!desc("name", None));

        // And an explicit direction wins.
        assert!(desc("serial", Some("desc")));
        assert!(!desc("created", Some("asc")));
    }

    #[test]
    fn an_unknown_direction_is_refused() {
        let err = FleetFilter {
            sort: Some("serial".into()),
            dir: Some("sideways".into()),
            ..Default::default()
        }
        .descending()
        .expect_err("must refuse");
        assert!(err.public_message().contains("asc or desc"));
    }

    #[test]
    fn a_never_seen_device_still_has_an_ordering_value() {
        // Without the COALESCE the tuple comparison goes NULL for these rows and they vanish from page two onwards.
        assert_eq!(
            FleetSort::LastSeen.expression("d"),
            "COALESCE(d.last_seen_at, 'epoch'::timestamptz)"
        );
        assert!(FleetSort::Serial.expression("d2").starts_with("d2."));
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
