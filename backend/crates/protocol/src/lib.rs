//! Wire types shared by the cloud, the vendor portal and every SDK.
//!
//! This crate has no opinion about storage, transport or auth. If a type here
//! needs a database or a broker to make sense, it belongs somewhere else.

#![forbid(unsafe_code)]

pub mod capability;
pub mod envelope;
pub mod thing_model;
pub mod topics;

/// The protocol version on the wire. Bumped only for breaking changes; new
/// optional fields do not bump it.
pub const WIRE_VERSION: u8 = 1;

/// Root of every topic.
pub const TOPIC_ROOT: &str = "haven";
