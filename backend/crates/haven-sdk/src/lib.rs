//! The device half of the Haven contract, as a library.
//!
//! A vendor does not write this twice. The same logic ships in this crate (for
//! reference and for anything Rust-shaped) and is the specification the C component
//! implements against. Everything that could be got wrong silently lives here:
//!
//! - **Topics** are built, never formatted by hand. A typo in a topic string is a
//!   device that connects, subscribes, and never hears a command.
//! - **Subscriptions survive reconnects.** A clean session comes back *deaf*, and a
//!   device that goes deaf after one broker blip is a device a customer reboots.
//! - **Envelopes** are the `protocol` crate's types, so a change to the wire format
//!   breaks the firmware build instead of the field.
//!
//! Deliberately not here: the device's policy. Which point is which, what a command
//! means for the hardware, how a firmware update is applied — that is the vendor's
//! product, and a library that guessed it would be a library nobody can use.

use std::collections::BTreeMap;

use protocol::envelope::{AckMsg, CommandMsg, CommandOp, DpValue, OtaReport, ShadowMsg};
use serde::Deserialize;
use uuid::Uuid;

pub mod claim;
pub mod client;
pub mod model;

pub use claim::{ClaimRequest, Claimed, claim};
pub use client::{DeviceClient, DeviceEvent, Link};
pub use model::{Point, PointSet};

/// Bumped when the wire format changes in a way a device must notice.
pub const PROTOCOL_VERSION: u8 = 1;

/// The firmware offer, as it arrives on the wire.
///
/// Declared here rather than imported from the platform's server types: a device and
/// the cloud agree on a *document*, not on a Rust type. `rollout_id` is accepted as an
/// alias because the first version of the field was named that.
#[derive(Debug, Clone, Deserialize)]
pub struct OtaOffer {
    #[serde(alias = "rollout_id")]
    pub campaign_id: Uuid,
    pub version: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub sha256: Option<String>,
}

/// What a device tells the platform about itself, right now.
#[derive(Debug, Clone)]
pub struct StateReport {
    pub points: BTreeMap<String, DpValue>,
    pub fw: String,
    pub online: bool,
    pub rssi: Option<i16>,
    pub battery: Option<u8>,
}

impl StateReport {
    pub fn from_points(points: &PointSet, fw: impl Into<String>) -> Self {
        Self {
            points: points.to_map(),
            fw: fw.into(),
            online: true,
            rssi: None,
            battery: None,
        }
    }

    pub fn shadow(&self, ts: i64) -> ShadowMsg {
        ShadowMsg {
            v: PROTOCOL_VERSION,
            ts,
            online: self.online,
            fw: Some(self.fw.clone()),
            rssi: self.rssi,
            pts: self.points.clone(),
        }
    }

    pub fn event(&self, ts: i64) -> protocol::envelope::EventMsg {
        protocol::envelope::EventMsg {
            v: PROTOCOL_VERSION,
            ts,
            seq: None,
            pts: self.points.clone(),
            batt: self.battery,
            rssi: self.rssi,
        }
    }
}

/// The outcome of a command, as the platform needs to hear it.
///
/// `Ok` with a value echoes the *new* state, which is what the app shows; `Err`
/// carries a code the UI can turn into a sentence ("blocked", "unsupported").
pub fn acknowledgement(command: &CommandMsg, points: &PointSet, ts: i64) -> AckMsg {
    AckMsg {
        v: PROTOCOL_VERSION,
        id: command.id,
        ts,
        ok: true,
        dp: command.dp.clone(),
        value: command.dp.as_deref().and_then(|dp| points.get(dp)).cloned(),
        code: None,
        msg: None,
    }
}

/// A refusal, with the reason a person will read.
pub fn refusal(command: &CommandMsg, ts: i64, code: &str, msg: &str) -> AckMsg {
    AckMsg {
        v: PROTOCOL_VERSION,
        id: command.id,
        ts,
        ok: false,
        dp: command.dp.clone(),
        value: None,
        code: Some(code.to_string()),
        msg: Some(msg.to_string()),
    }
}

/// How the flash is going.
pub fn ota_progress(campaign_id: Uuid, state: &str, pct: Option<u8>, ts: i64) -> OtaReport {
    OtaReport {
        v: PROTOCOL_VERSION,
        ts,
        campaign_id,
        state: state.to_string(),
        pct,
        code: None,
    }
}

/// A command that only asks for an action, never for a value.
pub fn is_action(command: &CommandMsg) -> bool {
    command.op == CommandOp::Action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(dp: Option<&str>, value: Option<DpValue>) -> CommandMsg {
        CommandMsg {
            v: 1,
            id: Uuid::from_u128(7),
            ts: 0,
            op: CommandOp::Set,
            dp: dp.map(str::to_string),
            value,
            action: None,
        }
    }

    #[test]
    fn an_ack_echoes_the_value_the_device_now_holds() {
        let mut points = PointSet::new();
        points.insert("1", DpValue::Bool(false));
        points.set("1", DpValue::Bool(true)).expect("known point");

        let ack = acknowledgement(&command(Some("1"), Some(DpValue::Bool(true))), &points, 42);
        assert!(ack.ok);
        assert_eq!(ack.value, Some(DpValue::Bool(true)));
        assert_eq!(ack.ts, 42);
        assert!(ack.code.is_none());
    }

    #[test]
    fn a_refusal_says_why_in_words_a_screen_can_show() {
        let ack = refusal(
            &command(Some("1"), None),
            42,
            "blocked",
            "the lamp is locked",
        );
        assert!(!ack.ok);
        assert_eq!(ack.code.as_deref(), Some("blocked"));
        assert_eq!(ack.msg.as_deref(), Some("the lamp is locked"));
        // A failed ack carries no value: echoing the old one would look like success.
        assert!(ack.value.is_none());
    }

    #[test]
    fn a_command_with_no_point_is_an_action_and_is_not_an_ack_target() {
        assert!(is_action(&CommandMsg {
            op: CommandOp::Action,
            ..command(None, None)
        }));
        assert!(!is_action(&command(Some("1"), Some(DpValue::Bool(true)))));
    }

    #[test]
    fn the_shadow_always_carries_the_firmware_version() {
        // "Up to date" is unreachable for a customer if a device never says what it
        // runs, and the shadow is where the platform looks.
        let mut points = PointSet::new();
        points.insert("1", DpValue::Bool(true));
        let report = StateReport::from_points(&points, "2.3.4");
        assert_eq!(report.shadow(1).fw.as_deref(), Some("2.3.4"));
        assert_eq!(report.shadow(1).v, PROTOCOL_VERSION);
    }

    #[test]
    fn an_offer_accepts_the_old_field_name_too() {
        // Devices in the field were handed `rollout_id` before it was renamed. A
        // firmware update that silently ignores the old name strands them.
        let json = serde_json::json!({
            "rollout_id": "01a1ce08-0000-7000-8000-000000000000",
            "version": "2.0.0"
        });
        let offer: OtaOffer = serde_json::from_value(json).expect("offer");
        assert_eq!(offer.version, "2.0.0");
        assert!(offer.size.is_none());
    }
}
