//! Message envelopes. Compact JSON, `v` first, unknown fields ignored so a
//! newer device firmware does not break an older cloud.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::WIRE_VERSION;

/// A datapoint value on the wire.
///
/// Untagged on purpose: a vendor's `80` is an integer and their `23.5` is a
/// float, and neither should require a tagged envelope in firmware.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DpValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl DpValue {
    /// JSON in, wire value out. Lives here rather than in the API because redelivery
    /// rebuilds envelopes from stored rows and must produce byte-identical packets.
    pub fn from_json(value: &serde_json::Value) -> Option<Self> {
        match value {
            serde_json::Value::Bool(b) => Some(DpValue::Bool(*b)),
            serde_json::Value::Number(n) => n
                .as_i64()
                .map(DpValue::Int)
                .or_else(|| n.as_f64().map(DpValue::Float)),
            serde_json::Value::String(s) => Some(DpValue::Text(s.clone())),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            DpValue::Int(i) => Some(*i as f64),
            DpValue::Float(f) => Some(*f),
            DpValue::Bool(b) => Some(u8::from(*b) as f64),
            DpValue::Text(s) => s.parse().ok(),
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            DpValue::Bool(b) => Some(*b),
            DpValue::Int(i) => Some(*i != 0),
            DpValue::Float(f) => Some(*f != 0.0),
            DpValue::Text(s) => match s.as_str() {
                "true" | "1" | "on" => Some(true),
                "false" | "0" | "off" => Some(false),
                _ => None,
            },
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            DpValue::Text(s) => Some(s),
            _ => None,
        }
    }
}

fn wire_v() -> u8 {
    WIRE_VERSION
}

/// `.../evt` — one or more changed datapoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMsg {
    #[serde(default = "wire_v")]
    pub v: u8,
    /// Epoch seconds, device clock.
    pub ts: i64,
    /// Monotonic per device, reset only on factory reset; used to detect loss.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u32>,
    /// Datapoint id → value.
    pub pts: BTreeMap<String, DpValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batt: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rssi: Option<i16>,
}

/// `.../ota` from the device: how far it has got with an offer.
///
/// The device speaks first here. A cloud that only sends offers and never
/// listens cannot tell a fleet that failed to download from one still working.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaReport {
    #[serde(default = "wire_v")]
    pub v: u8,
    /// Epoch seconds, device clock.
    pub ts: i64,
    /// The contract document calls this `rollout_id`; both names are accepted so
    /// firmware written against either one works.
    #[serde(alias = "rollout_id")]
    pub campaign_id: Uuid,
    /// `downloading` | `verified` | `rebooting` | `success` | `failed` | `skipped`.
    pub state: String,
    /// 0–100 while downloading. Optional: `success` has no percentage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pct: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// `.../shadow` — retained full state, and the Last-Will target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShadowMsg {
    #[serde(default = "wire_v")]
    pub v: u8,
    pub ts: i64,
    pub online: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fw: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rssi: Option<i16>,
    #[serde(default)]
    pub pts: BTreeMap<String, DpValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOp {
    /// Write a value.
    Set,
    /// Read a value back.
    Get,
    /// A named device action (`identify`, `calibrate`, …).
    Action,
}

/// `.../cmd` — cloud → device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMsg {
    #[serde(default = "wire_v")]
    pub v: u8,
    pub id: Uuid,
    pub ts: i64,
    pub op: CommandOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<DpValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

/// Stable ack codes. `msg` is for humans and may change; these may not.
pub mod ack_code {
    pub const INVALID_VALUE: &str = "invalid_value";
    pub const UNSUPPORTED_POINT: &str = "unsupported_point";
    pub const BUSY: &str = "busy";
    pub const UNAUTHORIZED: &str = "unauthorized";
    pub const NOT_READY: &str = "not_ready";
    pub const HARDWARE_FAULT: &str = "hardware_fault";
    pub const UNSUPPORTED_OP: &str = "unsupported_op";

    pub const ALL: &[&str] = &[
        INVALID_VALUE,
        UNSUPPORTED_POINT,
        BUSY,
        UNAUTHORIZED,
        NOT_READY,
        HARDWARE_FAULT,
        UNSUPPORTED_OP,
    ];
}

/// `.../ack` — exactly one per command id, exactly once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AckMsg {
    #[serde(default = "wire_v")]
    pub v: u8,
    pub id: Uuid,
    pub ts: i64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<DpValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OtaState {
    Downloading,
    Verified,
    Rebooting,
    Success,
    Failed,
}

/// OTA progress, device → cloud.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaProgress {
    #[serde(default = "wire_v")]
    pub v: u8,
    pub rollout_id: Uuid,
    pub state: OtaState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pct: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// The manifest, cloud → device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtaManifest {
    #[serde(default = "wire_v")]
    pub v: u8,
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    /// Base64 Ed25519 signature over the sha256 digest.
    pub sig: String,
    #[serde(default)]
    pub mandatory: bool,
    pub rollout_id: Uuid,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_round_trips_and_ignores_unknown_fields() {
        let raw = r#"{"v":1,"ts":1699999999,"seq":42,"pts":{"1":true,"2":80},
                      "future_field":{"a":1}}"#;
        let msg: EventMsg = serde_json::from_str(raw).expect("parses");
        assert_eq!(msg.pts["1"], DpValue::Bool(true));
        assert_eq!(msg.pts["2"], DpValue::Int(80));
        assert_eq!(msg.seq, Some(42));
    }

    #[test]
    fn numbers_keep_their_kind() {
        let v: DpValue = serde_json::from_str("80").unwrap();
        assert_eq!(v, DpValue::Int(80));
        let v: DpValue = serde_json::from_str("23.5").unwrap();
        assert_eq!(v, DpValue::Float(23.5));
        let v: DpValue = serde_json::from_str("\"#ff0000\"").unwrap();
        assert_eq!(v, DpValue::Text("#ff0000".into()));
    }

    #[test]
    fn command_serializes_compact_with_v_first() {
        let cmd = CommandMsg {
            v: 1,
            id: Uuid::nil(),
            ts: 1,
            op: CommandOp::Set,
            dp: Some("2".into()),
            value: Some(DpValue::Int(80)),
            action: None,
        };
        let s = serde_json::to_string(&cmd).unwrap();
        assert!(s.starts_with(r#"{"v":1,"#), "got {s}");
        assert!(s.contains(r#""op":"set""#));
    }

    #[test]
    fn shadow_offline_is_the_will_shape() {
        let raw = r#"{"v":1,"ts":1,"online":false}"#;
        let m: ShadowMsg = serde_json::from_str(raw).unwrap();
        assert!(!m.online);
        assert!(m.pts.is_empty());
    }
}
