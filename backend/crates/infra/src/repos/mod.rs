//! SQL implementations of the `domain` traits.
//!
//! One file per slice. These are the only places in the workspace that write SQL
//! for their slice, which is what keeps the rules testable without a database.

pub mod automation;
pub mod catalog;
pub mod devices;
pub mod homes;
pub mod identity;
pub mod ingest;
pub mod ota;
pub mod pairing;

pub use automation::PgAutomation;
pub use catalog::PgCatalog;
pub use devices::PgDevices;
pub use homes::PgHomes;
pub use identity::PgIdentity;
pub use ingest::PgIngest;
pub use pairing::PgPairing;
