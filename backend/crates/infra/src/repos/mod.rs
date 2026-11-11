//! SQL implementations of the `domain` traits.
//!
//! One file per slice. These are the only places in the workspace that write SQL
//! for their slice, which is what keeps the rules testable without a database.

pub mod devices;
pub mod homes;
pub mod identity;

pub use devices::PgDevices;
pub use homes::PgHomes;
pub use identity::PgIdentity;
