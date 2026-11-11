//! A product's thing model: which capabilities it has, and which vendor
//! datapoint backs each capability slot.
//!
//! This is data, not code. The portal validates it before publish, the claim
//! path expands it into `device_points`, and the app renders from the
//! capabilities — so a broken model is a broken product, and it is caught here.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::capability::{self, Access, History, Kind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThingModel {
    #[serde(default = "one")]
    pub schema_version: u16,
    pub product_key: String,
    pub name: String,
    pub category: String,
    #[serde(default)]
    pub icon: Option<String>,
    pub capabilities: Vec<CapabilityBinding>,
    #[serde(default)]
    pub constraints: Constraints,
}

fn one() -> u16 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityBinding {
    pub name: String,
    #[serde(default)]
    pub version: Option<u16>,
    /// slot name → vendor mapping.
    pub points: BTreeMap<String, PointMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointMapping {
    /// The vendor's datapoint id. Opaque to us.
    pub dp: String,
    pub kind: Kind,
    /// Multiplier applied vendor → platform (vendor 0..1000, platform 0..100).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    /// Vendor value → platform enum value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_map: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access: Option<Access>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraints {
    #[serde(default = "default_command_timeout")]
    pub command_timeout_ms: u32,
    #[serde(default)]
    pub min_firmware: Option<String>,
    #[serde(default)]
    pub allow_unknown_serial: bool,
}

fn default_command_timeout() -> u32 {
    10_000
}

impl Default for Constraints {
    fn default() -> Self {
        Self {
            command_timeout_ms: default_command_timeout(),
            min_firmware: None,
            allow_unknown_serial: false,
        }
    }
}

/// A rule that failed, with the rule number the vendor sees in the portal and
/// in `POST /v1/vendor/products/{id}/validate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleViolation {
    pub rule: u8,
    pub path: String,
    pub message: String,
}

impl RuleViolation {
    fn new(rule: u8, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule,
            path: path.into(),
            message: message.into(),
        }
    }
}

/// A point as it will exist on a device after claim.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPoint {
    /// `<capability>.<slot>`, e.g. `brightness.level`.
    pub code: String,
    pub capability: String,
    pub slot: String,
    pub dp: String,
    pub kind: Kind,
    pub access: Access,
    pub writable: bool,
    pub unit: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub scale: Option<f64>,
    pub enum_map: Option<BTreeMap<String, String>>,
    /// Copied from the capability catalog: the storage policy travels with the
    /// point so ingest does not have to look the capability up again.
    pub history: History,
}

impl ThingModel {
    /// Rules 1–6 from `docs/02-THING-MODEL.md`.
    pub fn validate(&self) -> Result<(), Vec<RuleViolation>> {
        let mut v = Vec::new();

        if self.product_key.trim().is_empty() {
            v.push(RuleViolation::new(
                0,
                "product_key",
                "product_key is required",
            ));
        }
        if self.capabilities.is_empty() {
            v.push(RuleViolation::new(
                0,
                "capabilities",
                "a product needs at least one capability",
            ));
        }

        let mut dp_kinds: HashMap<&str, Kind> = HashMap::new();
        let mut codes: HashMap<String, ()> = HashMap::new();

        for (i, binding) in self.capabilities.iter().enumerate() {
            let path = format!("capabilities[{i}]");

            // Rule 1 — the capability exists at a known version.
            let Some(cap) = (match binding.version {
                Some(ver) => capability::find_version(&binding.name, ver),
                None => capability::find(&binding.name),
            }) else {
                v.push(RuleViolation::new(
                    1,
                    format!("{path}.name"),
                    format!(
                        "unknown capability '{}'{}",
                        binding.name,
                        binding
                            .version
                            .map(|x| format!(" at version {x}"))
                            .unwrap_or_default()
                    ),
                ));
                continue;
            };

            // Rule 2 — every required slot is mapped, with a matching kind.
            for required in cap.slots {
                let Some(mapping) = binding.points.get(required.slot) else {
                    if required.access != Access::Write {
                        v.push(RuleViolation::new(
                            2,
                            format!("{path}.points.{}", required.slot),
                            format!("'{}' requires slot '{}'", cap.name, required.slot),
                        ));
                    }
                    continue;
                };

                if mapping.kind != required.kind {
                    v.push(RuleViolation::new(
                        2,
                        format!("{path}.points.{}", required.slot),
                        format!(
                            "'{}.{}' is {:?} in the catalog but mapped as {:?}",
                            cap.name, required.slot, required.kind, mapping.kind
                        ),
                    ));
                }

                // Rule 3 — one datapoint cannot mean two things.
                match dp_kinds.get(mapping.dp.as_str()) {
                    Some(existing) if *existing != mapping.kind => {
                        v.push(RuleViolation::new(
                            3,
                            format!("{path}.points.{}", required.slot),
                            format!(
                                "datapoint '{}' is claimed with two kinds ({:?} and {:?})",
                                mapping.dp, existing, mapping.kind
                            ),
                        ));
                    }
                    Some(_) => {}
                    None => {
                        dp_kinds.insert(mapping.dp.as_str(), mapping.kind);
                    }
                }

                // Rule 4 — an enum must say what its vendor values mean.
                if mapping.kind == Kind::Enum
                    && mapping.enum_map.as_ref().is_none_or(|m| m.is_empty())
                {
                    v.push(RuleViolation::new(
                        4,
                        format!("{path}.points.{}", required.slot),
                        format!("enum point '{}' needs an enum_map", mapping.dp),
                    ));
                }

                // Rule 5 — the vendor range must intersect the catalog range.
                if let Some(declared_max) = mapping.max
                    && let Some(catalog_max) = required.max
                    && declared_max > catalog_max * 1.0001
                {
                    v.push(RuleViolation::new(
                        5,
                        format!("{path}.points.{}", required.slot),
                        format!(
                            "{} max {declared_max} exceeds the catalog max {catalog_max}",
                            cap.name
                        ),
                    ));
                }
                if let Some(declared_min) = mapping.min
                    && let Some(catalog_min) = required.min
                    && declared_min < catalog_min * 1.0001
                {
                    v.push(RuleViolation::new(
                        5,
                        format!("{path}.points.{}", required.slot),
                        format!(
                            "{} min {declared_min} is below the catalog min {catalog_min}",
                            cap.name
                        ),
                    ));
                }

                let code = format!("{}.{}", cap.name, required.slot);
                if codes.insert(code.clone(), ()).is_some() {
                    v.push(RuleViolation::new(
                        3,
                        format!("{path}.points.{}", required.slot),
                        format!("duplicate point code '{code}'"),
                    ));
                }
            }

            // A slot the catalog does not have is a typo, not an extension.
            for slot in binding.points.keys() {
                if cap.slot(slot).is_none() {
                    v.push(RuleViolation::new(
                        2,
                        format!("{path}.points.{slot}"),
                        format!("'{}' has no slot '{slot}'", cap.name),
                    ));
                }
            }
        }

        // Rule 6 — the category's primary control must be present.
        let present: Vec<&str> = self.capabilities.iter().map(|c| c.name.as_str()).collect();
        for required in capability::required_for_category(&self.category) {
            if !present.contains(required) {
                v.push(RuleViolation::new(
                    6,
                    "capabilities",
                    format!(
                        "a '{}' product must expose '{required}' or it has no primary control",
                        self.category
                    ),
                ));
            }
        }

        if v.is_empty() { Ok(()) } else { Err(v) }
    }

    /// Expand the model into the point rows a claimed device gets.
    /// Panics only if called on an unvalidated model, which the claim path
    /// prevents by validating at publish time.
    pub fn resolve_points(&self) -> Vec<ResolvedPoint> {
        let mut out = Vec::new();
        for binding in &self.capabilities {
            let Some(cap) = capability::find(&binding.name) else {
                continue;
            };
            for slot in cap.slots {
                let Some(m) = binding.points.get(slot.slot) else {
                    continue;
                };
                out.push(ResolvedPoint {
                    code: format!("{}.{}", cap.name, slot.slot),
                    capability: cap.name.to_string(),
                    slot: slot.slot.to_string(),
                    dp: m.dp.clone(),
                    kind: m.kind,
                    access: m.access.unwrap_or(slot.access),
                    writable: m.access.unwrap_or(slot.access).can_write(),
                    unit: slot.unit.map(str::to_string),
                    min: m.min.or(slot.min),
                    max: m.max.or(slot.max),
                    scale: m.scale,
                    enum_map: m.enum_map.clone(),
                    history: slot.history,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bulb_without_switch() -> ThingModel {
        serde_json::from_value(serde_json::json!({
            "product_key": "hv_ref_bulb1",
            "name": "Reference Bulb",
            "category": "light",
            "capabilities": [
                { "name": "brightness",
                  "points": { "level": { "dp": "2", "kind": "int" } } }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn rule_6_rejects_a_light_without_a_switch() {
        let errs = bulb_without_switch().validate().unwrap_err();
        assert!(
            errs.iter().any(|e| e.rule == 6),
            "expected rule 6, got {errs:?}"
        );
    }

    #[test]
    fn a_complete_bulb_validates_and_resolves() {
        let model: ThingModel = serde_json::from_value(serde_json::json!({
            "product_key": "hv_ref_bulb1",
            "name": "Reference Bulb",
            "category": "light",
            "capabilities": [
                { "name": "switch", "points": { "power": { "dp": "1", "kind": "bool" } } },
                { "name": "brightness", "points": { "level": { "dp": "2", "kind": "int" } } }
            ]
        }))
        .unwrap();
        model.validate().expect("valid");
        let pts = model.resolve_points();
        assert_eq!(pts.len(), 2);
        let b = pts.iter().find(|p| p.code == "brightness.level").unwrap();
        assert!(b.writable);
        assert_eq!(b.min, Some(1.0));
        assert_eq!(b.unit.as_deref(), Some("%"));
    }

    #[test]
    fn rule_2_catches_a_kind_mismatch() {
        let model: ThingModel = serde_json::from_value(serde_json::json!({
            "product_key": "hv_x", "name": "X", "category": "light",
            "capabilities": [
                { "name": "switch", "points": { "power": { "dp": "1", "kind": "int" } } }
            ]
        }))
        .unwrap();
        let errs = model.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.rule == 2), "{errs:?}");
    }

    #[test]
    fn rule_3_catches_one_dp_two_meanings() {
        let model: ThingModel = serde_json::from_value(serde_json::json!({
            "product_key": "hv_x", "name": "X", "category": "plug",
            "capabilities": [
                { "name": "switch", "points": { "power": { "dp": "1", "kind": "bool" } } },
                { "name": "brightness", "points": { "level": { "dp": "1", "kind": "int" } } }
            ]
        }))
        .unwrap();
        let errs = model.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.rule == 3), "{errs:?}");
    }

    #[test]
    fn rule_4_requires_an_enum_map() {
        let model: ThingModel = serde_json::from_value(serde_json::json!({
            "product_key": "hv_x", "name": "X", "category": "thermostat",
            "capabilities": [
                { "name": "thermostat.mode", "points": { "mode": { "dp": "1", "kind": "enum" } } },
                { "name": "thermostat.target", "points": { "target": { "dp": "2", "kind": "float" } } }
            ]
        }))
        .unwrap();
        let errs = model.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.rule == 4), "{errs:?}");
    }

    #[test]
    fn rule_5_rejects_a_range_beyond_the_catalog() {
        let model: ThingModel = serde_json::from_value(serde_json::json!({
            "product_key": "hv_x", "name": "X", "category": "light",
            "capabilities": [
                { "name": "switch", "points": { "power": { "dp": "1", "kind": "bool" } } },
                { "name": "brightness",
                  "points": { "level": { "dp": "2", "kind": "int", "min": 0, "max": 10000 } } }
            ]
        }))
        .unwrap();
        let errs = model.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.rule == 5), "{errs:?}");
    }

    #[test]
    fn rule_1_rejects_an_unknown_capability() {
        let model: ThingModel = serde_json::from_value(serde_json::json!({
            "product_key": "hv_x", "name": "X", "category": "sensor",
            "capabilities": [{ "name": "telepathy", "points": {} }]
        }))
        .unwrap();
        let errs = model.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.rule == 1), "{errs:?}");
    }

    #[test]
    fn every_catalog_capability_can_be_mapped() {
        // Guards the catalog against a capability that can never satisfy rule 2.
        for cap in capability::catalog() {
            let points: BTreeMap<String, PointMapping> = cap
                .slots
                .iter()
                .map(|s| {
                    (
                        s.slot.to_string(),
                        PointMapping {
                            dp: format!("dp_{}", s.slot),
                            kind: s.kind,
                            scale: None,
                            enum_map: (s.kind == Kind::Enum)
                                .then(|| BTreeMap::from([("1".to_string(), "on".to_string())])),
                            min: None,
                            max: None,
                            access: None,
                        },
                    )
                })
                .collect();
            let model = ThingModel {
                schema_version: 1,
                product_key: "hv_test".into(),
                name: "T".into(),
                category: "sensor".into(),
                icon: None,
                capabilities: vec![CapabilityBinding {
                    name: cap.name.into(),
                    version: Some(cap.version),
                    points,
                }],
                constraints: Constraints::default(),
            };
            assert!(
                model.validate().is_ok(),
                "{} failed: {:?}",
                cap.name,
                model.validate()
            );
        }
    }
}
