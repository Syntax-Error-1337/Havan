//! The ingest lane.
//!
//! Parses the envelope, hands it to the one writer, and publishes the resulting
//! events for the API instances to fan out. A malformed payload is logged and
//! dropped: one bad device must never stall the lane for the rest of the fleet.

use domain::devices::{CommandRepo, Settled};
use domain::ota::{JobStatus, OtaRepo};
use std::sync::Arc;

use crate::automation::Runner;
use infra::Cache;
use infra::repos::ingest::PgIngest;
use infra::{PgDevices, PgOta};
use kernel::AppError;
use protocol::envelope::{AckMsg, EventMsg, OtaReport, ShadowMsg};
use protocol::topics::{DeviceTopic, Lane};
use serde_json::json;
use uuid::Uuid;

pub struct Ingest {
    writer: PgIngest,
    commands: PgDevices,
    cache: Cache,
    automations: Arc<Runner>,
    ota: PgOta,
}

impl Ingest {
    pub fn new(
        writer: PgIngest,
        commands: PgDevices,
        cache: Cache,
        automations: Arc<Runner>,
        ota: PgOta,
    ) -> Self {
        Self {
            writer,
            commands,
            cache,
            automations,
            ota,
        }
    }

    /// Route one message. Events go to the batch buffer, everything else is applied now:
    /// an ack settles a command the app is waiting on, and 20 ms of latency there is 20 ms
    /// the person sees.
    pub async fn on_message_batched(
        &self,
        topic: &DeviceTopic<'_>,
        payload: &[u8],
        batch: &mut Batch,
    ) {
        if !matches!(topic.lane, Lane::Evt) {
            self.on_message(topic, payload).await;
            return;
        }

        let Ok(device_id) = Uuid::parse_str(topic.device) else {
            tracing::warn!(device = %topic.device, "device segment is not a uuid");
            return;
        };
        match serde_json::from_slice::<EventMsg>(payload) {
            Ok(msg) => batch.push(device_id, msg),
            Err(e) => tracing::warn!(%device_id, error = %e, "evt rejected"),
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
            Lane::Ota => match serde_json::from_slice::<OtaReport>(payload) {
                Ok(report) => self.handle_ota(device_id, &report).await,
                Err(e) => tracing::warn!(%device_id, error = %e, "ota report rejected"),
            },
            Lane::Cmd => {}
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

    /// The device is telling us how the update is going.
    pub async fn handle_ota(&self, device_id: Uuid, report: &OtaReport) {
        let Some(state) = JobStatus::parse(&report.state) else {
            // Guessing would let a typo mark a fleet `success`.
            tracing::warn!(
                device = %device_id,
                state = %report.state,
                "unknown OTA state reported by a device"
            );
            return;
        };

        // `pending` and `sent` are the cloud's states. A device that reports one
        // is either confused or replaying an old message; either way, letting it
        // through would requeue a job that is already in flight.
        if matches!(state, JobStatus::Pending | JobStatus::Sent) {
            tracing::debug!(device = %device_id, "ignoring a cloud-owned OTA state");
            return;
        }

        match self
            .ota
            .mark_job(report.campaign_id, device_id, state, report.pct)
            .await
        {
            Ok(()) if state == JobStatus::Success => {
                // The device just became what it was offered. Recording it here is
                // what makes "up to date" reachable for the customer.
                match self
                    .ota
                    .record_installed(device_id, report.campaign_id)
                    .await
                {
                    Ok(Some(version)) => tracing::info!(
                        device = %device_id,
                        version = %version,
                        "device installed a new firmware"
                    ),
                    Ok(None) => tracing::warn!(
                        device = %device_id,
                        campaign = %report.campaign_id,
                        "a device reported success for a campaign it was not offered"
                    ),
                    Err(e) => tracing::warn!(error = ?e, "could not record the installed version"),
                }
            }
            Ok(()) => tracing::info!(
                campaign = %report.campaign_id,
                device = %device_id,
                state = %report.state,
                "OTA progress"
            ),
            Err(e) => tracing::warn!(error = ?e, "could not record OTA progress"),
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

/// Reports waiting to be written, grouped by device.
///
/// Grouped because the unit of work is a transaction: fifty reports for one device in one
/// transaction is one commit instead of fifty, which is what the ingest path spends its
/// time on (docs/08-PERFORMANCE.md). Two devices never share a transaction — a slow one
/// must not hold up the other.
#[derive(Default)]
pub struct Batch {
    pending: std::collections::HashMap<Uuid, Vec<infra::repos::ingest::Report>>,
    reports: usize,
}

impl Batch {
    /// How many reports are worth a transaction. Small enough that latency stays in the
    /// tens of milliseconds, large enough that a chatty device stops paying a commit a
    /// message.
    pub const CAPACITY: usize = 250;

    pub fn push(&mut self, device_id: Uuid, msg: EventMsg) {
        self.pending
            .entry(device_id)
            .or_default()
            .push(infra::repos::ingest::Report {
                ts: msg.ts,
                online: None,
                pts: msg.pts,
            });
        self.reports += 1;
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn len(&self) -> usize {
        self.reports
    }

    pub fn is_full(&self) -> bool {
        self.reports >= Self::CAPACITY
    }

    /// Write everything buffered and fan it out. Errors are logged per device: one bad
    /// device must not stop the rest, which is the same rule the single-message path had.
    pub async fn flush(&mut self, ingest: &Ingest) {
        if self.pending.is_empty() {
            return;
        }

        let pending = std::mem::take(&mut self.pending);
        self.reports = 0;

        for (device_id, reports) in pending {
            match ingest.writer.apply_reports(device_id, &reports).await {
                Ok(outcome) => ingest.publish(outcome).await,
                Err(e) => tracing::warn!(%device_id, error = ?e, "batch apply failed"),
            }
        }
    }
}
