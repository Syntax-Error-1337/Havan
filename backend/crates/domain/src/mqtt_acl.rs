//! Which topics a device may touch.
//!
//! Enforced by the broker through `vmq_webhooks` on the listener where a device authenticates
//! with a secret rather than a certificate. The rule is deliberately a single prefix match and
//! nothing else: a device owns `haven/<vendor>/<home>/<device>/…` and nothing outside it. Every
//! lane the contract defines — `shadow`, `evt`, `ack`, `cmd`, `ota` — sits under that prefix, so
//! a laneful of new topics does not need a new rule here, and a device cannot reach a sibling,
//! its home's other devices, or another tenant by naming them.

use crate::pairing::Authenticated;

/// The prefix every topic of one device starts with.
pub fn device_prefix(device: &Authenticated) -> String {
    format!(
        "haven/{}/{}/{}",
        device.vendor_id, device.home_id, device.device_id
    )
}

/// May this device publish to or subscribe on `topic`?
///
/// Wildcards are refused outright: `#` is a subscribe-all that would let one device read every
/// tenant, and `+` is the same power at one level less. A device has no reason to name either.
pub fn may_use_topic(device: &Authenticated, topic: &str) -> bool {
    if topic.contains('#') || topic.contains('+') {
        return false;
    }
    // `$SYS` and friends are broker-internal; a device has no business reading them.
    if topic.starts_with('$') {
        return false;
    }

    let prefix = device_prefix(device);
    let mut expected = prefix.split('/');
    let mut actual = topic.split('/');

    // Compare the four fixed segments exactly, then require at least one more: a device may use
    // `haven/v/h/d/evt` but not the bare `haven/v/h/d`.
    for _ in 0..4 {
        if expected.next() != actual.next() {
            return false;
        }
    }
    actual.next().is_some_and(|lane| !lane.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn device() -> Authenticated {
        Authenticated {
            device_id: Uuid::parse_str("11111111-1111-7111-8111-111111111111").expect("id"),
            vendor_id: Uuid::parse_str("22222222-2222-7222-8222-222222222222").expect("vendor"),
            home_id: Uuid::parse_str("33333333-3333-7333-8333-333333333333").expect("home"),
        }
    }

    #[test]
    fn every_lane_of_its_own_device_is_allowed() {
        let d = device();
        for lane in ["shadow", "evt", "ack", "cmd", "ota/progress"] {
            let topic = format!(
                "haven/{}/{}/{}/{}",
                d.vendor_id, d.home_id, d.device_id, lane
            );
            assert!(may_use_topic(&d, &topic), "{lane} should be allowed");
        }
    }

    #[test]
    fn another_device_in_the_same_home_is_not() {
        let d = device();
        let topic = format!(
            "haven/{}/{}/44444444-4444-7444-8444-444444444444/cmd",
            d.vendor_id, d.home_id
        );
        assert!(!may_use_topic(&d, &topic));
    }

    #[test]
    fn another_home_or_vendor_is_not() {
        let d = device();
        assert!(!may_use_topic(
            &d,
            &format!(
                "haven/{}/00000000-0000-7000-8000-000000000000/{}/evt",
                d.vendor_id, d.device_id
            )
        ));
        assert!(!may_use_topic(
            &d,
            &format!(
                "haven/00000000-0000-7000-8000-000000000000/{}/{}/evt",
                d.home_id, d.device_id
            )
        ));
    }

    #[test]
    fn wildcards_and_broker_topics_are_refused() {
        let d = device();
        assert!(!may_use_topic(&d, "haven/+/+/#"));
        assert!(!may_use_topic(
            &d,
            &format!("haven/{}/{}/+/cmd", d.vendor_id, d.home_id)
        ));
        assert!(!may_use_topic(&d, "$SYS/broker/uptime"));
    }

    #[test]
    fn the_bare_prefix_is_not_a_topic() {
        let d = device();
        assert!(!may_use_topic(&d, &device_prefix(&d)));
        assert!(!may_use_topic(&d, &format!("{}/", device_prefix(&d))));
    }
}
