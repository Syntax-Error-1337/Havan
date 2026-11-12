//! The device's datapoints, and what a command does to them.
//!
//! Pure: no broker, no clock, no database. This is the part of a device that is worth
//! testing on a laptop, and the part a C implementation must reproduce exactly.

use std::collections::BTreeMap;

use protocol::envelope::{CommandMsg, DpValue};

#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub dp: String,
    pub value: DpValue,
}

/// An ordered set of datapoints, keyed by DP id.
///
/// `BTreeMap`, not `HashMap`: two devices reporting the same state must produce
/// byte-identical payloads, and a hash map's iteration order would make the shadow
/// diff every time for no reason.
#[derive(Debug, Clone, Default)]
pub struct PointSet {
    points: BTreeMap<String, DpValue>,
}

impl PointSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_pairs(pairs: impl IntoIterator<Item = (impl Into<String>, DpValue)>) -> Self {
        let mut set = Self::new();
        for (dp, value) in pairs {
            set.insert(dp, value);
        }
        set
    }

    pub fn insert(&mut self, dp: impl Into<String>, value: DpValue) {
        self.points.insert(dp.into(), value);
    }

    pub fn get(&self, dp: &str) -> Option<&DpValue> {
        self.points.get(dp)
    }

    pub fn to_map(&self) -> BTreeMap<String, DpValue> {
        self.points.clone()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &DpValue)> {
        self.points.iter()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Apply a command. `Err` carries the code and message an ack must send back.
    ///
    /// The rules are deliberately strict, because a device that invents behaviour is a
    /// device the platform cannot predict:
    ///
    /// - an unknown datapoint is refused, not created;
    /// - the type must match the stored value (a bool point does not become a number);
    /// - a command with no value is a no-op that still succeeds — the platform uses it
    ///   to ask "are you there".
    pub fn set(&mut self, dp: &str, value: DpValue) -> Result<(), (String, String)> {
        let Some(current) = self.points.get_mut(dp) else {
            return Err((
                "unknown_dp".to_string(),
                format!("this device has no datapoint {dp}"),
            ));
        };

        if !same_kind(current, &value) {
            return Err((
                "bad_type".to_string(),
                format!("datapoint {dp} does not take that kind of value"),
            ));
        }

        *current = value;
        Ok(())
    }

    /// Apply whatever a command asks for, if it asks for anything.
    pub fn apply(&mut self, command: &CommandMsg) -> Result<(), (String, String)> {
        let (Some(dp), Some(value)) = (command.dp.as_deref(), command.value.clone()) else {
            return Ok(());
        };
        self.set(dp, value)
    }
}

/// DpValue is a wire type with a few numeric shapes; a device must not silently turn
/// a boolean point into a number because a bug sent `1` instead of `true`.
fn same_kind(a: &DpValue, b: &DpValue) -> bool {
    matches!(
        (a, b),
        (DpValue::Bool(_), DpValue::Bool(_))
            | (DpValue::Int(_), DpValue::Int(_))
            | (DpValue::Float(_), DpValue::Float(_))
            | (DpValue::Text(_), DpValue::Text(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::envelope::CommandOp;
    use uuid::Uuid;

    fn set(dp: Option<&str>, value: Option<DpValue>) -> CommandMsg {
        CommandMsg {
            v: 1,
            id: Uuid::from_u128(1),
            ts: 0,
            op: CommandOp::Set,
            dp: dp.map(str::to_string),
            value,
            action: None,
        }
    }

    fn bulb() -> PointSet {
        PointSet::from_pairs([("1", DpValue::Bool(false)), ("2", DpValue::Int(80))])
    }

    #[test]
    fn a_command_changes_the_point_it_names() {
        let mut points = bulb();
        points
            .apply(&set(Some("1"), Some(DpValue::Bool(true))))
            .expect("apply");
        assert_eq!(points.get("1"), Some(&DpValue::Bool(true)));
        assert_eq!(
            points.get("2"),
            Some(&DpValue::Int(80)),
            "other points untouched"
        );
    }

    #[test]
    fn an_unknown_datapoint_is_refused_rather_than_created() {
        let mut points = bulb();
        let err = points
            .apply(&set(Some("9"), Some(DpValue::Bool(true))))
            .expect_err("unknown dp");
        assert_eq!(err.0, "unknown_dp");
        assert_eq!(points.len(), 2, "the set did not grow");
    }

    #[test]
    fn the_wrong_kind_is_refused() {
        let mut points = bulb();
        let err = points
            .apply(&set(Some("1"), Some(DpValue::Int(1))))
            .expect_err("bool point");
        assert_eq!(err.0, "bad_type");
        assert_eq!(points.get("1"), Some(&DpValue::Bool(false)), "unchanged");
    }

    #[test]
    fn a_command_with_no_value_succeeds_and_changes_nothing() {
        let mut points = bulb();
        points.apply(&set(Some("1"), None)).expect("no-op");
        assert_eq!(points.get("1"), Some(&DpValue::Bool(false)));
    }

    #[test]
    fn the_same_state_always_serialises_the_same_way() {
        // Two devices in the same state must produce the same JSON, or every shadow
        // looks like a change and the platform rewrites rows that did not move.
        let a = PointSet::from_pairs([("2", DpValue::Int(80)), ("1", DpValue::Bool(true))]);
        let b = PointSet::from_pairs([("1", DpValue::Bool(true)), ("2", DpValue::Int(80))]);
        assert_eq!(
            serde_json::to_string(&a.to_map()).expect("a"),
            serde_json::to_string(&b.to_map()).expect("b")
        );
    }
}
