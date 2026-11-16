//! Feature routers. One module per slice; `app.rs` mounts them.

pub mod api_keys;
pub mod auth;
pub mod automations;
pub mod catalog;
pub mod certification;
pub mod devices;
pub mod history;
pub mod homes;
pub mod internal_mqtt;
pub mod licence;
pub mod notifications;
pub mod ops;
pub mod ota;
pub mod pairing;
pub mod privacy;
pub mod scenes;
pub mod stream;
pub mod vendor;
pub mod vendor_team;
