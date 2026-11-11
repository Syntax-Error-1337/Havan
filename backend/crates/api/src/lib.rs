//! The public HTTP surface. `main.rs` is the only place that reads the
//! environment; everything below takes its dependencies explicitly, which is
//! what makes an integration test a function call instead of a spawned process.

#![forbid(unsafe_code)]

pub mod app;
pub mod auth;
pub mod error;
pub mod health;
pub mod routes;

pub use app::{AppState, router};
