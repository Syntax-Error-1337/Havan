//! Over-the-air updates.
//!
//! A campaign is a rollout, not a broadcast. The rules that matter are the ones
//! that keep a bad firmware from reaching the whole fleet at once, so the
//! interesting logic here is *who is in this wave* — and it is deliberately a
//! pure function so it can be argued about without a database.

use std::future::Future;

use kernel::AppError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CampaignStatus {
    Draft,
    Running,
    Halted,
    Completed,
}

impl CampaignStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            CampaignStatus::Draft => "draft",
            CampaignStatus::Running => "running",
            CampaignStatus::Halted => "halted",
            // The column's check constraint allows `complete`, not `completed`.
            CampaignStatus::Completed => "complete",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Firmware {
    pub id: Uuid,
    pub product_id: Uuid,
    pub version: String,
    pub size: i64,
    pub sha256: String,
    /// Detached Ed25519 signature over the image, checked by the device before
    /// it writes a single byte to flash.
    pub ed25519_sig: String,
    pub object_key: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct Campaign {
    pub id: Uuid,
    pub vendor_id: Uuid,
    pub product_id: Uuid,
    pub firmware_id: Uuid,
    pub rollout_pct: i32,
    pub status: CampaignStatus,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub started_at: Option<time::OffsetDateTime>,
}

/// What the device is told over the OTA lane.
#[derive(Debug, Clone, Serialize)]
pub struct OtaOffer {
    pub v: u8,
    /// The SDK contract calls this `rollout_id`; both are accepted so firmware
    /// written against either document keeps working.
    #[serde(alias = "rollout_id")]
    pub campaign_id: Uuid,
    pub version: String,
    pub url: String,
    pub size: i64,
    pub sha256: String,
    pub sig: String,
}

/// Is this device in the rollout?
///
/// Percentages have to be *stable*: a device that flips in and out of a wave
/// every time the fleet is scanned would download, abort, and download again.
/// Hashing the pair (campaign, device) gives each device a fixed number in
/// 0..100 that never moves, and the boundary is a plain comparison.
pub fn in_rollout(campaign_id: Uuid, device_id: Uuid, pct: i32) -> bool {
    let pct = pct.clamp(0, 100);
    if pct == 0 {
        return false;
    }
    if pct == 100 {
        return true;
    }

    // FNV-1a over the one thing that must not change: the two ids.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in campaign_id
        .as_bytes()
        .iter()
        .chain(device_id.as_bytes().iter())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    (hash % 100) < pct as u64
}

/// What the *customer* is allowed to know about their device's firmware.
///
/// Deliberately tiny: the version, the outcome of the last attempt, and when.
/// Campaign ids, rollout percentages and other vendors' devices are none of a
/// customer's business, so this is a separate shape from `OtaJob` rather than a
/// projection of it.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceUpdate {
    pub current: Option<String>,
    pub target: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub at: Option<time::OffsetDateTime>,
    /// True while a job for this device is still moving.
    pub in_progress: bool,
}

impl DeviceUpdate {
    /// Nothing has ever been attempted, or the last attempt succeeded.
    pub fn is_settled(&self) -> bool {
        self.state.as_deref().is_none_or(|s| s == "success")
    }
}

pub trait OtaRepo {
    /// The customer-facing view of one device's firmware.
    ///
    /// Runs on a system context because `ota_jobs` is vendor-scoped and the caller
    /// is a customer; the route checks ownership before it gets here, and the
    /// result carries no vendor fields.
    fn device_update(
        &self,
        device_id: Uuid,
        current: Option<String>,
    ) -> impl Future<Output = Result<DeviceUpdate, AppError>> + Send;

    fn add_firmware(
        &self,
        vendor_id: Uuid,
        product_id: Uuid,
        firmware: &Firmware,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn firmware_for_product(
        &self,
        vendor_id: Uuid,
        product_id: Uuid,
    ) -> impl Future<Output = Result<Vec<Firmware>, AppError>> + Send;

    fn create_campaign(
        &self,
        vendor_id: Uuid,
        campaign: &Campaign,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn campaign(
        &self,
        vendor_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<Campaign>, AppError>> + Send;

    /// Every campaign for a vendor, newest first — the portal's table.
    fn campaigns(
        &self,
        vendor_id: Uuid,
    ) -> impl Future<Output = Result<Vec<Campaign>, AppError>> + Send;

    /// Start a campaign: decide who is in this wave and queue a job per device.
    /// Returns how many jobs were queued.
    /// A device reports a successful flash: record what it now runs.
    ///
    /// Without this, `devices.fw_version` stays whatever the device claimed with
    /// at pairing time, so a customer's device could never become "up to date" and
    /// the vendor's own fleet view lied about what was deployed.
    fn record_installed(
        &self,
        device_id: Uuid,
        campaign_id: Uuid,
    ) -> impl Future<Output = Result<Option<String>, AppError>> + Send;

    /// Stop a campaign. Returns how many queued jobs were skipped.
    ///
    /// In-flight jobs are left alone: a device that is half-way through a flash
    /// must finish or fail on its own schedule, and killing it mid-write is how
    /// firmware updates brick hardware.
    fn halt_campaign(
        &self,
        vendor_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = Result<u64, AppError>> + Send;

    fn start_campaign(
        &self,
        vendor_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = Result<u64, AppError>> + Send;

    /// Pending jobs, oldest first, with what is needed to send the offer.
    fn claim_jobs(&self, limit: i64) -> impl Future<Output = Result<Vec<OtaJob>, AppError>> + Send;

    fn progress(
        &self,
        vendor_id: Uuid,
        campaign_id: Uuid,
    ) -> impl Future<Output = Result<CampaignProgress, AppError>> + Send;

    /// Jobs that have gone quiet: requeued while attempts remain, failed after
    /// that. Returns how many were touched.
    fn sweep_stale(
        &self,
        stale_after_secs: i64,
        max_attempts: i32,
    ) -> impl Future<Output = Result<u64, AppError>> + Send;

    /// Close campaigns whose jobs are all final. Returns how many were closed.
    fn complete_campaigns(&self) -> impl Future<Output = Result<u64, AppError>> + Send;

    fn mark_job(
        &self,
        campaign_id: Uuid,
        device_id: Uuid,
        status: JobStatus,
        pct: Option<u8>,
    ) -> impl Future<Output = Result<(), AppError>> + Send;
}

/// The states `ota_jobs.state` allows, in the order a device walks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Sent,
    Downloading,
    Verified,
    Rebooting,
    Success,
    Failed,
    Skipped,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Pending => "pending",
            JobStatus::Sent => "sent",
            JobStatus::Downloading => "downloading",
            JobStatus::Verified => "verified",
            JobStatus::Rebooting => "rebooting",
            JobStatus::Success => "success",
            JobStatus::Failed => "failed",
            JobStatus::Skipped => "skipped",
        }
    }
}

impl JobStatus {
    /// A state reported by a device. Unknown strings are rejected rather than
    /// guessed at: a job quietly marked `success` because the string did not
    /// parse is how a fleet ends up half-updated and happy about it.
    pub fn parse(raw: &str) -> Option<JobStatus> {
        Some(match raw {
            "pending" => JobStatus::Pending,
            "sent" => JobStatus::Sent,
            "downloading" => JobStatus::Downloading,
            "verified" => JobStatus::Verified,
            "rebooting" => JobStatus::Rebooting,
            "success" => JobStatus::Success,
            "failed" => JobStatus::Failed,
            "skipped" => JobStatus::Skipped,
            _ => return None,
        })
    }

    /// Terminal states, so a campaign can be closed rather than watched forever.
    pub fn is_final(self) -> bool {
        matches!(
            self,
            JobStatus::Success | JobStatus::Failed | JobStatus::Skipped
        )
    }
}

/// How a campaign is going. Counts, not a status field: "running" says nothing
/// about whether ninety devices failed.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CampaignProgress {
    pub total: i64,
    pub pending: i64,
    pub sent: i64,
    pub downloading: i64,
    pub verified: i64,
    pub rebooting: i64,
    pub success: i64,
    pub failed: i64,
    pub skipped: i64,
}

impl CampaignProgress {
    pub fn finished(&self) -> i64 {
        self.success + self.failed + self.skipped
    }

    /// True once nothing is left in flight. A campaign with zero jobs is *not*
    /// complete: it matched nobody, which is a mistake to look at.
    pub fn is_complete(&self) -> bool {
        self.total > 0 && self.finished() == self.total
    }
}

#[derive(Debug, Clone)]
pub struct OtaJob {
    /// `ota_jobs` is keyed by (campaign, device): one offer per device per
    /// campaign, so there is no separate job id to mint or drift.
    pub campaign_id: Uuid,
    pub vendor_id: Uuid,
    /// `None` when the device has been released from its home — nothing to
    /// address, so the job is skipped rather than retried forever.
    pub home_id: Option<Uuid>,
    pub device_id: Uuid,
    pub version: String,
    pub sha256: String,
    pub sig: String,
    pub size: i64,
    pub object_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> Uuid {
        Uuid::from_u128(n as u128)
    }

    #[test]
    fn zero_and_full_rollouts_are_exact() {
        for n in 0..50u8 {
            assert!(!in_rollout(id(1), id(n), 0));
            assert!(in_rollout(id(1), id(n), 100));
        }
    }

    #[test]
    fn a_device_never_moves_between_waves() {
        // The whole point of hashing rather than sampling: calling it twice, or
        // after a restart, must give the same answer for the same device.
        for n in 0..200u8 {
            let first = in_rollout(id(7), id(n), 30);
            for _ in 0..5 {
                assert_eq!(first, in_rollout(id(7), id(n), 30));
            }
        }
    }

    #[test]
    fn a_higher_percentage_never_excludes_an_earlier_device() {
        // Monotonicity is what makes a staged rollout safe: going 5% → 50% must
        // only ever add devices, never drop one that already updated.
        for n in 0..200u8 {
            let mut seen = false;
            for pct in [0, 1, 5, 10, 25, 50, 75, 99, 100] {
                let included = in_rollout(id(3), id(n), pct);
                if seen {
                    assert!(included, "device {n} dropped out at {pct}%");
                }
                seen = included;
            }
        }
    }

    #[test]
    fn a_rollout_is_roughly_the_size_it_claims() {
        // Not a statistical guarantee — just a guard against a broken hash that
        // puts everything in one bucket.
        let total = 1000u32;
        for pct in [10, 30, 50] {
            let hits = (0..total)
                .filter(|n| {
                    let device = Uuid::from_u128(0x1000_0000 + u128::from(*n));
                    in_rollout(id(11), device, pct)
                })
                .count() as u32;
            let expected = total * pct as u32 / 100;
            let drift = hits.abs_diff(expected);
            assert!(
                drift < total / 10,
                "{pct}% rolled out {hits} of {total}, expected about {expected}"
            );
        }
    }

    #[test]
    fn different_campaigns_pick_different_devices() {
        // Otherwise wave 2 is just wave 1 again, and the second 50% never lands.
        let a: Vec<u8> = (0..100u8)
            .filter(|n| in_rollout(id(1), id(*n), 50))
            .collect();
        let b: Vec<u8> = (0..100u8)
            .filter(|n| in_rollout(id(2), id(*n), 50))
            .collect();
        assert_ne!(a, b);
    }
}
