//! Everything that talks to the outside world: Postgres, Redis, MQTT, object
//! storage. `domain` defines the traits; this crate is the only implementation.

#![forbid(unsafe_code)]

pub mod mqtt;
pub mod pg;
pub mod redis;
pub mod repos;
pub mod store;

pub use mqtt::Mqtt;
pub use pg::Pg;
pub use redis::Cache;
pub use repos::api_key::PgApiKeys;
pub use repos::fleet::PgFleet;
pub use repos::history::PgHistory;
pub use repos::notification::PgNotifications;
pub use repos::ota::PgOta;
pub use repos::scenes::PgScenes;
pub use repos::serials::PgSerials;
pub use repos::{PgAutomation, PgCatalog, PgDevices, PgHomes, PgIdentity, PgIngest, PgPairing};
pub use store::Objects;
