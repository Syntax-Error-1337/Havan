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

/// How long a device may sit on an offer before the job is considered lost.
/// Long enough for a slow download on a bad link, short enough that a vendor
/// watching a rollout sees something happen within the hour.
const STALE_AFTER_SECS: i64 = 45 * 60;

/// Total attempts per device, including the first. Three tries over two hours is
/// generous for a firmware that may need a reboot; beyond that the device is not
/// coming back and the vendor should hear about it.
const MAX_ATTEMPTS: i32 = 3;

pub struct Dispatcher {
    repo: PgOta,
    mqtt: AsyncClient,
}

impl Dispatcher {
    pub fn new(repo: PgOta, mqtt: AsyncClient) -> Self {
        Self { repo, mqtt }
    }

    pub async fn tick(&self) {
        // Sweep before claiming: a job that has gone quiet should be back in the
        // queue by the time this tick looks for work.
        match self.repo.sweep_stale(STALE_AFTER_SECS, MAX_ATTEMPTS).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(jobs = n, "swept stale OTA jobs"),
            Err(e) => tracing::warn!(error = ?e, "OTA sweep failed"),
        }

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
                        .mark_job(job.campaign_id, job.device_id, JobStatus::Skipped, None)
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
                        .mark_job(job.campaign_id, job.device_id, JobStatus::Pending, None)
                        .await;
                }
            }

            // Breathing room between publishes: a campaign that matches two
            // thousand devices should not hand the broker a burst.
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// Close campaigns that have nothing left in flight.
    pub async fn settle_campaigns(&self) {
        match self.repo.complete_campaigns().await {
            Ok(0) => {}
            Ok(n) => tracing::info!(campaigns = n, "closed finished OTA campaigns"),
            Err(e) => tracing::warn!(error = ?e, "could not close OTA campaigns"),
        }
    }

    pub fn spawn(self, interval: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(interval);
            loop {
                tick.tick().await;
                self.tick().await;
                self.settle_campaigns().await;
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
