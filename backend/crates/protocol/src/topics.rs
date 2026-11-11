//! Topic construction and parsing.
//!
//! Nothing else in the workspace may format a topic by hand: a typo here is a
//! silent data loss, and the broker ACL is written against these strings.

use crate::TOPIC_ROOT;

/// The five lanes a device owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lane {
    /// State and telemetry reports, device → cloud.
    Evt,
    /// Full state, retained, also the Last-Will target.
    Shadow,
    /// Commands, cloud → device.
    Cmd,
    /// One ack per command id.
    Ack,
    /// Firmware manifest down, progress up.
    Ota,
}

impl Lane {
    pub const fn as_str(self) -> &'static str {
        match self {
            Lane::Evt => "evt",
            Lane::Shadow => "shadow",
            Lane::Cmd => "cmd",
            Lane::Ack => "ack",
            Lane::Ota => "ota",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "evt" => Lane::Evt,
            "shadow" => Lane::Shadow,
            "cmd" => Lane::Cmd,
            "ack" => Lane::Ack,
            "ota" => Lane::Ota,
            _ => return None,
        })
    }
}

/// A parsed device topic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceTopic<'a> {
    pub vendor: &'a str,
    pub home: &'a str,
    pub device: &'a str,
    pub lane: Lane,
}

fn build(vendor: &str, home: &str, device: &str, lane: Lane) -> String {
    format!("{TOPIC_ROOT}/v1/{vendor}/{home}/{device}/{}", lane.as_str())
}

pub fn evt(vendor: &str, home: &str, device: &str) -> String {
    build(vendor, home, device, Lane::Evt)
}
pub fn shadow(vendor: &str, home: &str, device: &str) -> String {
    build(vendor, home, device, Lane::Shadow)
}
pub fn cmd(vendor: &str, home: &str, device: &str) -> String {
    build(vendor, home, device, Lane::Cmd)
}
pub fn ack(vendor: &str, home: &str, device: &str) -> String {
    build(vendor, home, device, Lane::Ack)
}
pub fn ota(vendor: &str, home: &str, device: &str) -> String {
    build(vendor, home, device, Lane::Ota)
}

/// Shared subscription the ingest workers use. `$share/<group>/<filter>` means
/// the broker delivers each message to exactly one worker in the group, which is what
/// makes horizontal scaling safe.
pub const INGEST_SHARED_FILTER: &str = "$share/ingest/haven/v1/+/+/+/+";

/// Returns `None` for a topic outside the contract, including our own shared
/// subscription form — callers parse what they receive, not what they sent.
pub fn parse(topic: &str) -> Option<DeviceTopic<'_>> {
    let rest = topic.strip_prefix(TOPIC_ROOT)?.strip_prefix("/v1/")?;
    let mut parts = rest.split('/');
    let vendor = parts.next()?;
    let home = parts.next()?;
    let device = parts.next()?;
    let lane = Lane::parse(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    if vendor.is_empty() || home.is_empty() || device.is_empty() {
        return None;
    }
    Some(DeviceTopic {
        vendor,
        home,
        device,
        lane,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // The exact strings from docs/03-SDK-CONTRACT.md §4.2.
    #[test]
    fn builders_match_the_contract() {
        assert_eq!(evt("acme", "h1", "d1"), "haven/v1/acme/h1/d1/evt");
        assert_eq!(shadow("acme", "h1", "d1"), "haven/v1/acme/h1/d1/shadow");
        assert_eq!(cmd("acme", "h1", "d1"), "haven/v1/acme/h1/d1/cmd");
        assert_eq!(ack("acme", "h1", "d1"), "haven/v1/acme/h1/d1/ack");
        assert_eq!(ota("acme", "h1", "d1"), "haven/v1/acme/h1/d1/ota");
    }

    #[test]
    fn round_trips() {
        for lane in [Lane::Evt, Lane::Shadow, Lane::Cmd, Lane::Ack, Lane::Ota] {
            let t = build("v", "h", "d", lane);
            let p = parse(&t).expect("parses");
            assert_eq!((p.vendor, p.home, p.device, p.lane), ("v", "h", "d", lane));
        }
    }

    #[test]
    fn rejects_foreign_and_malformed_topics() {
        assert!(parse("homeassistant/v1/a/b/c/evt").is_none());
        assert!(parse("haven/v1/a/b/c").is_none());
        assert!(parse("haven/v1/a/b/c/evt/extra").is_none());
        assert!(parse("haven/v1/a/b/c/nope").is_none());
        assert!(parse("haven/v1//b/c/evt").is_none());
        // The shared-subscription form is not a valid received topic.
        assert!(parse(INGEST_SHARED_FILTER).is_none());
    }
}
