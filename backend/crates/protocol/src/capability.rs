//! The platform's capability vocabulary.
//!
//! A vendor maps their datapoints onto these; the app renders these. Adding a
//! capability is a platform release, never a vendor action — that constraint is
//! what keeps one app binary able to draw thousands of SKUs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Bool,
    Int,
    Float,
    Text,
    Enum,
    Raw,
}

impl Kind {
    /// The value stored in `device_points.kind`. Stable: it is written to the
    /// database, so change it with a migration, not a rename.
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Bool => "bool",
            Kind::Int => "int",
            Kind::Float => "float",
            Kind::Text => "text",
            Kind::Enum => "enum",
            Kind::Raw => "raw",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Device reports, cloud never writes.
    Read,
    /// Cloud writes, device does not report (rare: an action).
    Write,
    ReadWrite,
}

impl Access {
    pub fn can_write(self) -> bool {
        matches!(self, Access::Write | Access::ReadWrite)
    }
    pub fn can_read(self) -> bool {
        matches!(self, Access::Read | Access::ReadWrite)
    }
}

/// Whether a datapoint is worth storing a time series for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum History {
    /// No telemetry at all; only the current value.
    Off,
    /// A row whenever the value changes (the default for switches).
    Change,
    /// A row every N seconds even if unchanged (climate sensors).
    Interval(u32),
    /// Every report (debugging only; expensive).
    All,
}

impl History {
    /// Stored in `device_points.history`. Stable text, not debug output.
    pub fn as_str(self) -> String {
        match self {
            History::Off => "off".to_string(),
            History::Change => "change".to_string(),
            History::Interval(secs) => format!("interval:{secs}"),
            History::All => "all".to_string(),
        }
    }

    pub fn parse(s: &str) -> History {
        match s {
            "off" => History::Off,
            "all" => History::All,
            other => match other.strip_prefix("interval:") {
                Some(n) => History::Interval(n.parse().unwrap_or(300)),
                None => History::Change,
            },
        }
    }

    pub fn stores_rows(self) -> bool {
        !matches!(self, History::Off)
    }
}

/// One value a capability exposes.
#[derive(Debug, Clone, Copy)]
pub struct SlotDef {
    pub slot: &'static str,
    pub kind: Kind,
    pub access: Access,
    pub unit: Option<&'static str>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub history: History,
}

#[derive(Debug, Clone, Copy)]
pub struct Capability {
    pub name: &'static str,
    pub version: u16,
    pub slots: &'static [SlotDef],
    /// Which automation comparisons make sense (`eq`, `gt`, …).
    pub comparable: bool,
    /// Whether an automation may write this capability.
    pub settable: bool,
    pub category: &'static str,
    pub widget: &'static str,
}

const fn slot(
    slot: &'static str,
    kind: Kind,
    access: Access,
    unit: Option<&'static str>,
    min: Option<f64>,
    max: Option<f64>,
    history: History,
) -> SlotDef {
    SlotDef {
        slot,
        kind,
        access,
        unit,
        min,
        max,
        history,
    }
}

const SWITCH: &[SlotDef] = &[slot(
    "power",
    Kind::Bool,
    Access::ReadWrite,
    None,
    None,
    None,
    History::Change,
)];
const BRIGHTNESS: &[SlotDef] = &[slot(
    "level",
    Kind::Int,
    Access::ReadWrite,
    Some("%"),
    Some(1.0),
    Some(100.0),
    // On change, not `off`: brightness is the one thing a person sets and later
    // wants to look at ("why is it dim in here at 6pm?"). A bulb changes it a few
    // times a day, so the rows are nothing.
    History::Change,
)];
const COLOR_TEMP: &[SlotDef] = &[slot(
    "kelvin",
    Kind::Int,
    Access::ReadWrite,
    Some("K"),
    Some(2000.0),
    Some(6500.0),
    History::Off,
)];
const COLOR_RGB: &[SlotDef] = &[slot(
    "rgb",
    Kind::Text,
    Access::ReadWrite,
    None,
    None,
    None,
    History::Off,
)];
const COVER_POSITION: &[SlotDef] = &[slot(
    "position",
    Kind::Int,
    Access::ReadWrite,
    Some("%"),
    Some(0.0),
    Some(100.0),
    History::Change,
)];
const COVER_CONTROL: &[SlotDef] = &[slot(
    "command",
    Kind::Enum,
    Access::Write,
    None,
    None,
    None,
    History::Off,
)];
const FAN_SPEED: &[SlotDef] = &[slot(
    "level",
    Kind::Int,
    Access::ReadWrite,
    Some("%"),
    Some(0.0),
    Some(100.0),
    History::Off,
)];
const FAN_MODE: &[SlotDef] = &[slot(
    "mode",
    Kind::Enum,
    Access::ReadWrite,
    None,
    None,
    None,
    History::Change,
)];
const THERMOSTAT_MODE: &[SlotDef] = &[slot(
    "mode",
    Kind::Enum,
    Access::ReadWrite,
    None,
    None,
    None,
    History::Change,
)];
const THERMOSTAT_TARGET: &[SlotDef] = &[slot(
    "target",
    Kind::Float,
    Access::ReadWrite,
    Some("°C"),
    Some(5.0),
    Some(35.0),
    History::Change,
)];
const THERMOSTAT_CURRENT: &[SlotDef] = &[slot(
    "value",
    Kind::Float,
    Access::Read,
    Some("°C"),
    Some(-40.0),
    Some(80.0),
    History::Interval(300),
)];
const LOCK: &[SlotDef] = &[slot(
    "locked",
    Kind::Bool,
    Access::ReadWrite,
    None,
    None,
    None,
    History::Change,
)];
const TEMPERATURE: &[SlotDef] = &[slot(
    "value",
    Kind::Float,
    Access::Read,
    Some("°C"),
    Some(-40.0),
    Some(80.0),
    History::Interval(300),
)];
const HUMIDITY: &[SlotDef] = &[slot(
    "value",
    Kind::Float,
    Access::Read,
    Some("%"),
    Some(0.0),
    Some(100.0),
    History::Interval(300),
)];
const CONTACT: &[SlotDef] = &[slot(
    "open",
    Kind::Bool,
    Access::Read,
    None,
    None,
    None,
    History::Change,
)];
const MOTION: &[SlotDef] = &[slot(
    "detected",
    Kind::Bool,
    Access::Read,
    None,
    None,
    None,
    History::Change,
)];
const BATTERY: &[SlotDef] = &[slot(
    "level",
    Kind::Int,
    Access::Read,
    Some("%"),
    Some(0.0),
    Some(100.0),
    History::Change,
)];
const POWER: &[SlotDef] = &[slot(
    "watts",
    Kind::Float,
    Access::Read,
    Some("W"),
    Some(0.0),
    None,
    History::Interval(60),
)];
const ENERGY_TOTAL: &[SlotDef] = &[slot(
    "kwh",
    Kind::Float,
    Access::Read,
    Some("kWh"),
    Some(0.0),
    None,
    History::Interval(3600),
)];
const MEDIA_PLAYBACK: &[SlotDef] = &[slot(
    "state",
    Kind::Enum,
    Access::ReadWrite,
    None,
    None,
    None,
    History::Change,
)];
const MEDIA_VOLUME: &[SlotDef] = &[slot(
    "level",
    Kind::Int,
    Access::ReadWrite,
    Some("%"),
    Some(0.0),
    Some(100.0),
    History::Off,
)];
const CAMERA_STREAM: &[SlotDef] = &[slot(
    "url",
    Kind::Text,
    Access::Read,
    None,
    None,
    None,
    History::Off,
)];
const RSSI: &[SlotDef] = &[slot(
    "value",
    Kind::Int,
    Access::Read,
    Some("dBm"),
    Some(-120.0),
    Some(0.0),
    History::Off,
)];

/// Every capability the platform knows. Ordered by name for `catalog()` lookup.
pub const CATALOG: &[Capability] = &[
    Capability {
        name: "battery",
        version: 1,
        slots: BATTERY,
        comparable: true,
        settable: false,
        category: "sensor",
        widget: "badge",
    },
    Capability {
        name: "brightness",
        version: 1,
        slots: BRIGHTNESS,
        comparable: true,
        settable: true,
        category: "light",
        widget: "slider",
    },
    Capability {
        name: "camera.stream",
        version: 1,
        slots: CAMERA_STREAM,
        comparable: false,
        settable: false,
        category: "camera",
        widget: "player",
    },
    Capability {
        name: "color_rgb",
        version: 1,
        slots: COLOR_RGB,
        comparable: false,
        settable: true,
        category: "light",
        widget: "color_wheel",
    },
    Capability {
        name: "color_temp",
        version: 1,
        slots: COLOR_TEMP,
        comparable: true,
        settable: true,
        category: "light",
        widget: "warm_slider",
    },
    Capability {
        name: "contact",
        version: 1,
        slots: CONTACT,
        comparable: true,
        settable: false,
        category: "sensor",
        widget: "tile",
    },
    Capability {
        name: "cover.control",
        version: 1,
        slots: COVER_CONTROL,
        comparable: false,
        settable: true,
        category: "cover",
        widget: "transport",
    },
    Capability {
        name: "cover.position",
        version: 1,
        slots: COVER_POSITION,
        comparable: true,
        settable: true,
        category: "cover",
        widget: "vertical_slider",
    },
    Capability {
        name: "energy.power",
        version: 1,
        slots: POWER,
        comparable: true,
        settable: false,
        category: "energy",
        widget: "sparkline",
    },
    Capability {
        name: "energy.total",
        version: 1,
        slots: ENERGY_TOTAL,
        comparable: true,
        settable: false,
        category: "energy",
        widget: "number",
    },
    Capability {
        name: "fan.mode",
        version: 1,
        slots: FAN_MODE,
        comparable: true,
        settable: true,
        category: "fan",
        widget: "segmented",
    },
    Capability {
        name: "fan.speed",
        version: 1,
        slots: FAN_SPEED,
        comparable: true,
        settable: true,
        category: "fan",
        widget: "segmented",
    },
    Capability {
        name: "humidity",
        version: 1,
        slots: HUMIDITY,
        comparable: true,
        settable: false,
        category: "sensor",
        widget: "tile",
    },
    Capability {
        name: "lock",
        version: 1,
        slots: LOCK,
        comparable: true,
        settable: true,
        category: "lock",
        widget: "lock_button",
    },
    Capability {
        name: "media.playback",
        version: 1,
        slots: MEDIA_PLAYBACK,
        comparable: true,
        settable: true,
        category: "media",
        widget: "transport",
    },
    Capability {
        name: "media.volume",
        version: 1,
        slots: MEDIA_VOLUME,
        comparable: true,
        settable: true,
        category: "media",
        widget: "slider",
    },
    Capability {
        name: "motion",
        version: 1,
        slots: MOTION,
        comparable: true,
        settable: false,
        category: "sensor",
        widget: "tile",
    },
    Capability {
        name: "rssi",
        version: 1,
        slots: RSSI,
        comparable: true,
        settable: false,
        category: "diagnostics",
        widget: "hidden",
    },
    Capability {
        name: "switch",
        version: 1,
        slots: SWITCH,
        comparable: true,
        settable: true,
        category: "power",
        widget: "toggle",
    },
    Capability {
        name: "temperature",
        version: 1,
        slots: TEMPERATURE,
        comparable: true,
        settable: false,
        category: "sensor",
        widget: "tile",
    },
    Capability {
        name: "thermostat.current",
        version: 1,
        slots: THERMOSTAT_CURRENT,
        comparable: true,
        settable: false,
        category: "climate",
        widget: "readout",
    },
    Capability {
        name: "thermostat.mode",
        version: 1,
        slots: THERMOSTAT_MODE,
        comparable: true,
        settable: true,
        category: "climate",
        widget: "chips",
    },
    Capability {
        name: "thermostat.target",
        version: 1,
        slots: THERMOSTAT_TARGET,
        comparable: true,
        settable: true,
        category: "climate",
        widget: "dial",
    },
];

pub fn catalog() -> &'static [Capability] {
    CATALOG
}

pub fn find(name: &str) -> Option<&'static Capability> {
    CATALOG.iter().find(|c| c.name == name)
}

pub fn find_version(name: &str, version: u16) -> Option<&'static Capability> {
    CATALOG
        .iter()
        .find(|c| c.name == name && c.version == version)
}

impl Capability {
    pub fn slot(&self, slot: &str) -> Option<&'static SlotDef> {
        self.slots.iter().find(|s| s.slot == slot)
    }
}

/// Capabilities a product with this category must expose, or its main control is
/// missing and pairing will look broken to the customer.
pub fn required_for_category(category: &str) -> &'static [&'static str] {
    match category {
        "light" => &["switch"],
        "plug" | "appliance" => &["switch"],
        "cover" => &["cover.position"],
        "lock" => &["lock"],
        "thermostat" => &["thermostat.mode", "thermostat.target"],
        "camera" => &["camera.stream"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_sorted_and_unique() {
        let mut names: Vec<_> = CATALOG.iter().map(|c| c.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), CATALOG.len());
        for c in CATALOG {
            assert!(!c.slots.is_empty(), "{} has no slots", c.name);
            let mut slots: Vec<_> = c.slots.iter().map(|s| s.slot).collect();
            slots.sort_unstable();
            slots.dedup();
            assert_eq!(slots.len(), c.slots.len(), "{} has duplicate slots", c.name);
        }
    }

    #[test]
    fn every_settable_capability_has_a_writable_slot() {
        for c in CATALOG.iter().filter(|c| c.settable) {
            assert!(
                c.slots.iter().any(|s| s.access.can_write()),
                "{} is settable but has no writable slot",
                c.name
            );
        }
    }

    #[test]
    fn bounded_slots_are_bounded_in_order() {
        for c in CATALOG {
            for s in c.slots {
                if let (Some(min), Some(max)) = (s.min, s.max) {
                    assert!(min < max, "{}.{} has min >= max", c.name, s.slot);
                }
            }
        }
    }
}
