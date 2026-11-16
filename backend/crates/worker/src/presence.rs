//! Presence events — the one shape the app understands for "this device is reachable".
//!
//! Two things decide presence: a report arriving (ingest) and a report *stopping* (the sweep).
//! They used to be able to disagree: ingest published `device.online` / `device.offline`, while the
//! sweep only moved the row, so an app with an open socket kept showing a dead bulb as online until
//! somebody pulled to refresh. Both build the frame here now, so they cannot drift apart again, and
//! the announcement lives here rather than in `main` so a test can prove it.

use serde_json::json;
use uuid::Uuid;

/// The frame published on `home:{home_id}` when a device's reachability changes.
pub fn event(online: bool, home_id: Uuid, device_id: Uuid) -> String {
    json!({
        "t": if online { "device.online" } else { "device.offline" },
        "home_id": home_id,
        "device_id": device_id,
        "ts": kernel::now_s(),
    })
    .to_string()
}

/// Tell every home that a device it owns has gone quiet.
///
/// Returns how many announcements went out. A publish failure stops the pass instead of being
/// swallowed: if the broker or Redis is unreachable, retrying a hundred times a second only makes
/// the same failure louder, and the next sweep will announce whoever is still silent.
pub async fn announce_silent(cache: &infra::Cache, silent: &[infra::SilentDevice]) -> usize {
    let mut sent = 0;
    for d in silent {
        let channel = format!("home:{}", d.home_id);
        let body = event(false, d.home_id, d.device_id);
        if let Err(e) = cache.publish_event(&channel, &body).await {
            tracing::warn!(device = %d.device_id, error = ?e, "presence announcement failed");
            break;
        }
        sent += 1;
    }
    sent
}
