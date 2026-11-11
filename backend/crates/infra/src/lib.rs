//! Everything that talks to the outside world: Postgres, Redis, MQTT, object
//! storage. `domain` defines the traits; this crate is the only implementation.

#![forbid(unsafe_code)]

pub mod mqtt;
pub mod pg;
pub mod redis;
pub mod store;

pub use mqtt::Mqtt;
pub use pg::Pg;
pub use redis::Cache;
pub use store::Objects;
