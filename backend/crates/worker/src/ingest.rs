//! The ingest lane.
//!
//! Phase 0 validates the envelope and counts. The writers (current value,
//! telemetry, presence, fan-out) attach here in Task 10 without changing the
//! subscription or the supervisor loop.

use protocol::envelope::{AckMsg, EventMsg, ShadowMsg};
use protocol::topics::DeviceTopic;
use tracing::debug;

/// Called once per published message, already identified by topic.
///
/// A malformed payload is logged and dropped: one bad device must never stall
/// the lane for the rest of the fleet.
pub async fn on_message(topic: &DeviceTopic<'_>, payload: &[u8]) {
    match topic.lane {
        protocol::topics::Lane::Evt => match serde_json::from_slice::<EventMsg>(payload) {
            Ok(msg) => debug!(device = %topic.device, points = msg.pts.len(), "evt"),
            Err(e) => debug!(device = %topic.device, error = %e, "evt rejected"),
        },
        protocol::topics::Lane::Shadow => match serde_json::from_slice::<ShadowMsg>(payload) {
            Ok(msg) => debug!(device = %topic.device, online = msg.online, "shadow"),
            Err(e) => debug!(device = %topic.device, error = %e, "shadow rejected"),
        },
        protocol::topics::Lane::Ack => match serde_json::from_slice::<AckMsg>(payload) {
            Ok(msg) => debug!(device = %topic.device, ok = msg.ok, "ack"),
            Err(e) => debug!(device = %topic.device, error = %e, "ack rejected"),
        },
        // OTA progress is parsed by the campaign slice; commands are never
        // published to us.
        protocol::topics::Lane::Ota | protocol::topics::Lane::Cmd => {}
    }
}
