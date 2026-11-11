//! Feature routers. One module per slice; `app.rs` mounts them.

pub mod auth;
pub mod automations;
pub mod catalog;
pub mod devices;
pub mod homes;
pub mod pairing;
pub mod stream;
pub mod vendor;
