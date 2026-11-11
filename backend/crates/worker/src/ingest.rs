//! The ingest lane.
//!
//! Parses the envelope, hands it to the one writer, and publishes the resulting
//! events for the API instances to fan out. A malformed payload is logged and
//! dropped: one bad device must never stall the lane for the rest of the fleet.

use infra::Cache;
use infra::repos::ingest::PgIngest;
use kernel::AppError;
use protocol::envelope::{AckMsg, EventMsg, ShadowMsg};
use protocol::topics::{DeviceTopic, Lane};
use serde_json::json;
use uuid::Uuid;

pub struct Ingest {
    writer: PgIngest,
    cache: Cache,
}

impl Ingest {
    pub fn new(writer: PgIngest, cache: Cache) -> Self {
        Self { writer, cache }
    }

    pub async fn on_message(&self, topic: &DeviceTopic<'_>, payload: &[u8]) {
        // Device identity comes from the topic, never from the payload.
        let Ok(device_id) = Uuid::parse_str(topic.device) else {
            tracing::warn!(device = %topic.device, "device segment is not a uuid");
            return;
        };

        match topic.lane {
            Lane::Evt => match serde_json::from_slice::<EventMsg>(payload) {
                Ok(msg) => self.apply_event(device_id, &msg).await,
                Err(e) => tracing::warn!(%device_id, error = %e, "evt rejected"),
            },
            Lane::Shadow => match serde_json::from_slice::<ShadowMsg>(payload) {
                Ok(msg) => self.apply_shadow(device_id, &msg).await,
                Err(e) => tracing::warn!(%device_id, error = %e, "shadow rejected"),
            },
            Lane::Ack => match serde_json::from_slice::<AckMsg>(payload) {
                Ok(msg) => {
                    // Commands are settled by the command slice; until it lands,
                    // log it so the round trip is observable in a demo.
                    tracing::info!(%device_id, ok = msg.ok, id = %msg.id, "ack");
                }
                Err(e) => tracing::warn!(%device_id, error = %e, "ack rejected"),
            },
            Lane::Ota | Lane::Cmd => {}
        }
    }

    async fn apply_event(&self, device_id: Uuid, msg: &EventMsg) {
        match self.writer.apply_event(device_id, msg).await {
            Ok(outcome) => self.publish(outcome).await,
            Err(e) => warn_apply(device_id, e),
        }
    }

    async fn apply_shadow(&self, device_id: Uuid, msg: &ShadowMsg) {
        match self.writer.apply_shadow(device_id, msg).await {
            Ok(outcome) => self.publish(outcome).await,
            Err(e) => warn_apply(device_id, e),
        }
    }

    async fn publish(&self, outcome: infra::repos::ingest::IngestOutcome) {
        let Some(home_id) = outcome.home_id else {
            // A released device still reports; nothing to fan out.
            return;
        };

        let channel = format!("home:{home_id}");

        for update in &outcome.updated {
            let event = json!({
                "t": "point.changed",
                "home_id": home_id,
                "device_id": update.device_id,
                "code": update.code,
                "value": update.value,
                "ts": update.ts,
            });
            if let Err(e) = self.cache.publish_event(&channel, &event.to_string()).await {
                tracing::warn!(error = ?e, "fan-out publish failed");
            }
        }

        let presence = json!({
            "t": if outcome.online { "device.online" } else { "device.offline" },
            "home_id": home_id,
            "device_id": outcome.device_id,
            "ts": kernel::now_s(),
        });
        let _ = self
            .cache
            .publish_event(&channel, &presence.to_string())
            .await;
    }
}

fn warn_apply(device_id: Uuid, e: AppError) {
    tracing::warn!(%device_id, error = ?e, "ingest failed; dropping this report");
}
