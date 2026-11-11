//! Cross-cutting primitives: configuration, errors, ids, the tenant context.
//!
//! This crate deliberately knows nothing about HTTP, SQL or MQTT. If it needs a
//! framework to compile, the type belongs in `infra` or `api`.

#![forbid(unsafe_code)]

pub mod config;
pub mod context;
pub mod error;
pub mod ids;
pub mod telemetry;

pub use config::Config;
pub use context::{Actor, TenantCtx};
pub use error::{AppError, ErrorCode};
pub use ids::new_id;
