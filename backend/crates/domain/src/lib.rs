//! Use cases. No HTTP, no SQL, no MQTT: each slice defines the traits it needs
//! and `infra` implements them, so the rules can be tested with fakes.
//!
//! Slices land here one at a time, each with `mod.rs` (public surface),
//! `model.rs`, `repo.rs` (trait), `service.rs`, `error.rs`.

#![forbid(unsafe_code)]

pub mod homes;
pub mod identity;

