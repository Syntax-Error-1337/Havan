//! The ingest lane.
//!
//! Parses the envelope, hands it to the one writer, and publishes the resulting
//! events for the API instances to fan out. A malformed payload is logged and
//! dropped: one bad device must never stall the lane for the rest of the fleet.

use domain::devices::{CommandRepo, Settled};
use std::sync::Arc;

use crate::automation::Runner;
use infra::Cache;
use infra::PgDevices;
use infra::repos::ingest::PgIngest;
use kernel::AppError;
use protocol::envelope::{AckMsg, EventMsg, ShadowMsg};
use protocol::topics::{DeviceTopic, Lane};
use serde_json::json;
use uuid::Uuid;

pub struct Ingest {
    writer: PgIngest,
    commands: PgDevices,
    cache: Cache,
    automations: Arc<Runner>,
}

impl Ingest {
    pub fn new(
        writer: PgIngest,
        commands: PgDevices,
        cache: Cache,
        automations: Arc<Runner>,
    ) -> Self {
        Self {
            writer,
            commands,
            cache,
            automations,
        }
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
                Ok(msg) => self.handle_ack(msg).await,
                Err(e) => tracing::warn!(%device_id, error = %e, "ack rejected"),
            },
            Lane::Ota | Lane::Cmd => {}
        }
    }

    /// The device has spoken: close the command, then tell every open app.
    ///
    /// Public because it is the seam a test needs: everything else in this lane
    /// is a decode step, and this is the one that changes state and fans out.
    pub async fn handle_ack(&self, ack: AckMsg) {
        let reported = serde_json::json!({ "dp": ack.dp, "value": ack.value });

        match self
            .commands
            .settle_from_ack(ack.id, ack.ok, reported, ack.code.as_deref())
            .await
        {
            Ok(Some(settled)) => {
                tracing::info!(command = %settled.command_id, status = %settled.status, "settled");
                self.publish_settled(&settled).await;
            }
            // Unknown id or already settled: a redelivered ack must not
            // resurrect a command that timed out.
            Ok(None) => tracing::debug!(command = %ack.id, "ack for a settled or unknown command"),
            Err(e) => tracing::warn!(error = ?e, "could not settle ack"),
        }
    }

    pub async fn publish_settled(&self, settled: &Settled) {
        let Some(home_id) = settled.home_id else {
            return;
        };
        let event = serde_json::json!({
            "t": "command.settled",
            "home_id": home_id,
            "device_id": settled.device_id,
            "command_id": settled.command_id,
            "status": settled.status,
            "ts": kernel::now_s(),
        });
        let channel = format!("home:{home_id}");
        let _ = self.cache.publish_event(&channel, &event.to_string()).await;
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

            // Automation dispatch happens after the write and after the fan-out:
            // a rule that fires late is a nuisance, a rule that fires before the
            // state is readable is a bug. Only a real change fires it — a device
            // that re-reports its shadow every 30 s must not re-run the rule.
            if update.changed {
                self.automations
                    .on_event(
                        update.device_id,
                        &update.code,
                        &serde_json::to_value(&update.value).unwrap_or_default(),
                    )
                    .await;
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
