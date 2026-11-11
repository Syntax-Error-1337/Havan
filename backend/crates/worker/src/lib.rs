//! The worker, as a library.
//!
//! The binary is thin on purpose: the ingest lane and the scheduler are the
//! interesting parts, and they can only be tested if they are reachable from a
//! test. `haven-worker` wires them to MQTT.

pub mod automation;
pub mod ingest;
