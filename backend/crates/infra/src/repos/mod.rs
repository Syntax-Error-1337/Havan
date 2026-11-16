//! SQL implementations of the `domain` traits.
//!
//! One file per slice. These are the only places in the workspace that write SQL
//! for their slice, which is what keeps the rules testable without a database.

pub mod api_key;
pub mod audit;
pub mod automation;
pub mod catalog;
pub mod cert_queue;
pub mod conformance;
pub mod devices;
pub mod fleet;
pub mod history;
pub mod homes;
pub mod identity;
pub mod ingest;
pub mod licence;
pub mod notification;
pub mod ota;
pub mod pairing;
pub mod privacy;
pub mod scenes;
pub mod serials;
pub mod sso;
pub mod support;
pub mod vendor_admin;
pub mod vendor_security;
pub mod vendor_team;
pub mod webhook;

pub use audit::PgAudit;
pub use automation::PgAutomation;
pub use catalog::PgCatalog;
pub use devices::PgDevices;
pub use homes::PgHomes;
pub use identity::PgIdentity;
pub use ingest::PgIngest;
pub use pairing::PgPairing;
