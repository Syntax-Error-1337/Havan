//! `haven-sim` — a virtual device, written against the SDK a vendor would use.
//!
//! This file is deliberately thin: claim, decide what the product's datapoints mean,
//! and react to what the SDK hands back. Everything that could be got wrong silently —
//! topics, re-subscribing after a reconnect, ack shape, OTA progress — lives in
//! `haven-sdk` and is tested there. If this file grew a protocol detail again, the SDK
//! would be incomplete.
//!
//! ```text
//! HAVEN_PK=hv_acme_bulb1 HAVEN_SN=SN-1 HAVEN_SECRET=… HAVEN_TICKET=… cargo run -p simulator
//! ```

use std::time::Duration;

use anyhow::Context;
use haven_sdk::{
    ClaimRequest, DeviceClient, DeviceEvent, Link, PointSet, StateReport, acknowledgement, claim,
    ota_progress, refusal,
};
use protocol::envelope::DpValue;

/// The reference bulb: DP 1 = power, 2 = brightness, 3 = colour temperature. A real
/// firmware derives these from the product's thing model; the simulator hard-codes them
/// because it is standing in for one specific product.
const FIRMWARE: &str = "1.0.0";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let api = env_or("HAVEN_API", "http://127.0.0.1:8080");
    let mqtt_url = env_or("HAVEN_MQTT", "mqtt://127.0.0.1:1883");

    let request = ClaimRequest {
        product_key: std::env::var("HAVEN_PK").context("HAVEN_PK is required")?,
        serial: std::env::var("HAVEN_SN").context("HAVEN_SN is required")?,
        secret: std::env::var("HAVEN_SECRET").context("HAVEN_SECRET is required")?,
        ticket: std::env::var("HAVEN_TICKET").context("HAVEN_TICKET is required")?,
        firmware: FIRMWARE.to_string(),
    };

    let claimed = claim(&api, &request).await?;
    tracing::info!(
        device = %claimed.device_id,
        home = ?claimed.home_id,
        points = claimed.points,
        "claimed"
    );

    let link = Link::from_claim(claimed.vendor_id, claimed.home_id, claimed.device_id);
    let mut points = PointSet::from_pairs([
        ("1", DpValue::Bool(false)),
        ("2", DpValue::Int(80)),
        ("3", DpValue::Int(4000)),
    ]);

    let will = DeviceClient::offline_will(&link, FIRMWARE);
    let mut client = DeviceClient::connect(
        link.clone(),
        &mqtt_url,
        &claimed.device_id.to_string(),
        Some(will),
    )
    .await?;

    // A device reports on a cadence as well as on change: a shadow that stops updating
    // is indistinguishable from a device that is alive but idle.
    let mut heartbeat = tokio::time::interval(Duration::from_secs(30));
    heartbeat.tick().await;

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                let report = StateReport::from_points(&points, FIRMWARE);
                if let Err(e) = client.publish_shadow(&report, kernel::now_s()).await {
                    // A device does not exit because the network blinked.
                    tracing::warn!(error = %e, "shadow publish failed; staying up");
                }
            }
            event = client.next() => match event {
                DeviceEvent::Connected => {
                    tracing::info!(topics = ?link.subscriptions(), "connected; subscribed");
                    // The shadow is retained, so a device that reconnects replaces a
                    // stale one immediately rather than 30 seconds later.
                    let report = StateReport::from_points(&points, FIRMWARE);
                    client.publish_shadow(&report, kernel::now_s()).await?;
                }
                DeviceEvent::Disconnected(reason) => {
                    tracing::warn!(%reason, "link dropped; retrying");
                }
                DeviceEvent::Command(command) => {
                    let ts = kernel::now_s();
                    match points.apply(&command) {
                        Ok(()) => {
                            let report = StateReport::from_points(&points, FIRMWARE);
                            client.publish_state(&report, ts).await?;
                            client
                                .publish_ack(&acknowledgement(&command, &points, ts))
                                .await?;
                            tracing::info!(cmd = %command.id, op = ?command.op, "acked");
                        }
                        Err((code, message)) => {
                            // The device refuses in words the app can show: "unknown
                            // datapoint" beats a silent no-op.
                            client
                                .publish_ack(&refusal(&command, ts, &code, &message))
                                .await?;
                            tracing::warn!(cmd = %command.id, %code, %message, "refused");
                        }
                    }
                }
                DeviceEvent::OtaOffer(offer) => {
                    tracing::info!(version = %offer.version, "firmware offered");
                    // A real device downloads, verifies, reboots, then reports. The
                    // steps are reported one at a time so the vendor's rollout table
                    // shows progress instead of a jump from queued to done.
                    for (state, pct, wait) in [
                        ("downloading", Some(60u8), 400u64),
                        ("verified", None, 300),
                        ("rebooting", None, 400),
                        ("success", None, 0),
                    ] {
                        client
                            .report_ota(&ota_progress(offer.campaign_id, state, pct, kernel::now_s()))
                            .await?;
                        if wait > 0 {
                            tokio::time::sleep(Duration::from_millis(wait)).await;
                        }
                    }
                    // The new firmware is what it now runs; saying so is what makes
                    // "up to date" reachable for the customer.
                    tracing::info!(version = %offer.version, "installed");
                }
            },
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
