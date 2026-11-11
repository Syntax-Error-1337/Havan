//! The OTA dispatcher.
//!
//! It does one thing: take jobs that are queued and tell the devices about
//! them. No image bytes move here — the device downloads from object storage and
//! checks the signature itself.

use std::time::Duration;

use domain::ota::{JobStatus, OtaOffer, OtaRepo};
use infra::PgOta;
use protocol::topics;
use rumqttc::{AsyncClient, QoS};

pub struct Dispatcher {
    repo: PgOta,
    mqtt: AsyncClient,
}

impl Dispatcher {
    pub fn new(repo: PgOta, mqtt: AsyncClient) -> Self {
        Self { repo, mqtt }
    }

    pub async fn tick(&self) {
        let jobs = match self.repo.claim_jobs(32).await {
            Ok(jobs) => jobs,
            Err(e) => {
                tracing::warn!(error = ?e, "could not claim OTA jobs");
                return;
            }
        };

        for job in jobs {
            let topic = match job.topic() {
                Some(topic) => topic,
                None => {
                    // A device with no home cannot be addressed. Skip it loudly:
                    // otherwise the job is re-claimed every tick forever.
                    tracing::warn!(
                        campaign = %job.campaign_id,
                        device = %job.device_id,
                        "OTA job for a device with no home"
                    );
                    let _ = self
                        .repo
                        .mark_job(job.campaign_id, job.device_id, JobStatus::Skipped)
                        .await;
                    continue;
                }
            };

            let offer = OtaOffer {
                v: 1,
                campaign_id: job.campaign_id,
                version: job.version.clone(),
                url: job.object_key.clone(),
                size: job.size,
                sha256: job.sha256.clone(),
                sig: job.sig.clone(),
            };

            let payload = match serde_json::to_vec(&offer) {
                Ok(payload) => payload,
                Err(e) => {
                    tracing::warn!(error = ?e, "OTA offer did not serialise");
                    continue;
                }
            };

            match self
                .mqtt
                .publish(&topic, QoS::AtLeastOnce, false, payload)
                .await
            {
                Ok(()) => {
                    tracing::info!(
                        campaign = %job.campaign_id,
                        device = %job.device_id,
                        version = %offer.version,
                        "OTA offer sent"
                    );
                }
                Err(e) => {
                    // Put it back so the next tick retries; a lost offer is a
                    // device that never updates, and nobody would notice.
                    tracing::warn!(error = ?e, device = %job.device_id, "OTA offer failed");
                    let _ = self
                        .repo
                        .mark_job(job.campaign_id, job.device_id, JobStatus::Pending)
                        .await;
                }
            }

            // Breathing room between publishes: a campaign that matches two
            // thousand devices should not hand the broker a burst.
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    pub fn spawn(self, interval: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            loop {
                tick.tick().await;
                self.tick().await;
            }
        })
    }
}

/// The offer needs the device's own topic, which is a function of vendor, home
/// and device. Kept here rather than in the domain type so the domain stays
/// free of broker knowledge.
pub trait JobTopic {
    fn topic(&self) -> Option<String>;
}

impl JobTopic for domain::ota::OtaJob {
    fn topic(&self) -> Option<String> {
        let home = self.home_id?;
        Some(topics::ota(
            &self.vendor_id.to_string(),
            &home.to_string(),
            &self.device_id.to_string(),
        ))
    }
}
