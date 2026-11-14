//! What the C SDK emits, parsed by the server that has to read it.
//!
//! This test exists because the two sides disagreed for as long as nothing compared them. `sdk/c`
//! built `haven/<vendor>/<home>/<device>/<lane>` while the worker subscribes to
//! `haven/v1/+/+/+/+`; its shadow said `on` where `ShadowMsg` requires `online`; its ack omitted
//! `ok`, which `AckMsg` requires. All three passed 45 checks in the C suite, because those checks
//! compared the SDK against itself. A real ESP8266 published into that gap for an hour: connected,
//! publishing, and rejected as malformed by the worker, which is indistinguishable from a device
//! that sends nothing.
//!
//! The rule this enforces: a device SDK is not correct because its own tests pass, it is correct
//! because the server can parse what it produces.
//!
//! Skipped when the C test binary has not been built (`make sdk` builds it). A skip is visible in
//! the output — a silent pass here would be the same mistake in a different costume.

use std::path::PathBuf;
use std::process::Command;

use protocol::envelope::{AckMsg, EventMsg, ShadowMsg};

/// Where `make -C sdk/c test` leaves the compiled test binary, which also has a `--emit` mode.
fn c_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../sdk/c/tests/test_haven")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from("sdk/c/tests/test_haven"))
}

/// Every envelope the C SDK emits, keyed by the line prefix it prints.
fn emitted() -> Option<Vec<(String, String)>> {
    let bin = c_binary();
    if !bin.is_file() {
        eprintln!(
            "skipping: {} is not built — run `make sdk` first, then this test compares the C SDK \
             against the server's own parsers",
            bin.display()
        );
        return None;
    }
    let out = Command::new(&bin)
        .arg("--emit")
        .output()
        .expect("run the C SDK emission mode");
    assert!(
        out.status.success(),
        "the C SDK's --emit mode failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let text = String::from_utf8(out.stdout).expect("utf-8 output");
    let pairs = text
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(kind, rest)| (kind.to_string(), rest.trim().to_string()))
        .collect::<Vec<_>>();
    assert!(!pairs.is_empty(), "the C SDK emitted nothing");
    Some(pairs)
}

fn get<'a>(pairs: &'a [(String, String)], kind: &str) -> &'a str {
    pairs
        .iter()
        .find(|(k, _)| k == kind)
        .map(|(_, v)| v.as_str())
        .unwrap_or_else(|| panic!("the C SDK emitted no `{kind}` line"))
}

#[test]
fn the_topic_a_device_builds_is_the_topic_the_server_parses() {
    let Some(pairs) = emitted() else { return };

    let vendor = "11111111-1111-7111-8111-111111111111";
    let home = "22222222-2222-7222-8222-222222222222";
    let device = "33333333-3333-7333-8333-333333333333";

    // Four lanes, and each has to parse — not merely look right. `haven/<v>/<h>/<d>/evt` is
    // well-formed and unparseable, which is exactly the bug this catches.
    let topics: Vec<&str> = pairs
        .iter()
        .filter(|(k, _)| k == "topic")
        .map(|(_, v)| v.as_str())
        .collect();
    assert_eq!(topics.len(), 4, "expected evt, shadow, ack and cmd");

    for topic in topics {
        let parsed = protocol::topics::parse(topic)
            .unwrap_or_else(|| panic!("the server cannot parse the SDK's topic: {topic}"));
        assert_eq!(parsed.vendor, vendor, "vendor in {topic}");
        assert_eq!(parsed.home, home, "home in {topic}");
        assert_eq!(parsed.device, device, "device in {topic}");
    }
}

#[test]
fn the_shadow_a_device_builds_deserialises_as_a_shadow() {
    let Some(pairs) = emitted() else { return };
    let raw = get(&pairs, "shadow");

    let msg: ShadowMsg = serde_json::from_str(raw)
        .unwrap_or_else(|e| panic!("the server cannot parse the SDK's shadow ({e}): {raw}"));
    assert!(msg.online, "the shadow must say `online`, not `on`");
    assert_eq!(msg.fw.as_deref(), Some("1.0.0"));
    assert_eq!(msg.pts.len(), 2);
    assert_eq!(msg.ts, 1_700_000_000);
}

#[test]
fn the_event_a_device_builds_deserialises_as_an_event() {
    let Some(pairs) = emitted() else { return };
    let raw = get(&pairs, "event");

    let msg: EventMsg = serde_json::from_str(raw)
        .unwrap_or_else(|e| panic!("the server cannot parse the SDK's event ({e}): {raw}"));
    assert_eq!(msg.pts.len(), 2, "a report with no points is not a report");
}

#[test]
fn the_ack_and_the_refusal_both_deserialise_with_their_verdict() {
    let Some(pairs) = emitted() else { return };

    let ack: AckMsg = serde_json::from_str(get(&pairs, "ack"))
        .unwrap_or_else(|e| panic!("the server cannot parse the SDK's ack ({e})"));
    assert!(ack.ok, "an ack that is not `ok: true` settles as a failure");

    let refusal: AckMsg = serde_json::from_str(get(&pairs, "refusal"))
        .unwrap_or_else(|e| panic!("the server cannot parse the SDK's refusal ({e})"));
    assert!(!refusal.ok);
    assert_eq!(refusal.code.as_deref(), Some("bad_type"));
}
