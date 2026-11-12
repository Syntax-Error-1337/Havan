//! Claiming: the one moment a device uses its factory secret.
//!
//! Everything after this is the broker. Kept separate so firmware can replace the
//! transport (a phone relays the claim over BLE, a factory jig uses a serial line)
//! without touching the rest of the SDK.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

/// What the factory put in the device.
#[derive(Debug, Clone)]
pub struct ClaimRequest {
    pub product_key: String,
    pub serial: String,
    /// The per-device secret. Hashed by the platform on first claim; never reused.
    pub secret: String,
    /// The pairing ticket, from the app that is adding this device.
    pub ticket: String,
    pub firmware: String,
}

/// Where this device should connect, once the platform has accepted it.
#[derive(Debug, Clone, Deserialize)]
pub struct Claimed {
    pub device_id: Uuid,
    pub vendor_id: Uuid,
    /// Absent when the ticket was minted by a claim that is not yet in a home.
    #[serde(default)]
    pub home_id: Option<Uuid>,
    #[serde(default)]
    pub name: Option<String>,
    /// Number of datapoints the product declares, for a device that wants to check its
    /// own model against the platform's.
    #[serde(default)]
    pub points: usize,
    #[serde(default)]
    pub mqtt: Option<Broker>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Broker {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default = "default_keepalive")]
    pub keepalive: u16,
}

fn default_keepalive() -> u16 {
    30
}

/// Claim this device. Idempotent on the platform side: a device that reboots mid-claim
/// and retries gets the same `device_id` back.
pub async fn claim(api_base: &str, request: &ClaimRequest) -> Result<Claimed> {
    let body = json!({
        "pk": request.product_key,
        "sn": request.serial,
        "secret": request.secret,
        "ticket": request.ticket,
        "fw": request.firmware,
    });

    let response = reqwest::Client::new()
        .post(format!("{api_base}/v1/provision/claim"))
        .json(&body)
        .send()
        .await
        .context("claim request")?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        // The platform's message is written for a person; do not replace it with
        // "request failed".
        anyhow::bail!("claim refused ({status}): {text}");
    }

    response.json().await.context("claim response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_claim_response_may_omit_the_home_and_the_broker() {
        // A factory jig claims before anybody has a home; the device must still be
        // able to parse the answer rather than crash on a missing field.
        let claimed: Claimed = serde_json::from_value(serde_json::json!({
            "device_id": "01a1ce08-0000-7000-8000-000000000001",
            "vendor_id": "01a1ce08-0000-7000-8000-000000000002",
            "points": 3
        }))
        .expect("minimal claim");
        assert_eq!(claimed.points, 3);
        assert!(claimed.home_id.is_none());
        assert!(claimed.mqtt.is_none());
    }

    #[test]
    fn a_broker_block_defaults_the_keepalive() {
        let broker: Broker = serde_json::from_value(serde_json::json!({
            "host": "mqtt.example", "port": 8883
        }))
        .expect("broker");
        assert_eq!(broker.keepalive, 30);
    }
}
