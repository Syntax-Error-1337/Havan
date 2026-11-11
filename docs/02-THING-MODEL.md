# Thing model — products, capabilities, datapoints

The thing model is the contract between three parties: the vendor's firmware, our
cloud, and our app. If it is wrong, the app renders the wrong control and no
amount of code fixes it fast.

## Three levels

```
Product          hv_acme_bulb1        "Acme Bulb E27"
 └ Capability    switch, brightness   platform-owned vocabulary (the "what it does")
    └ Point      power → dp "1"       vendor-owned wire detail (the "how it says it")
```

- **Capability** is a platform concept. There are finitely many, they are
  versioned, and only the platform may add one. The app renders capabilities.
- **Point** is the vendor's datapoint (Tuya calls it a DP). Firmware sends points;
  the cloud maps a point to a capability slot.
- **Product** is a capability list plus a point mapping. Data, not code.

The consequence: a vendor adding a new bulb model changes JSON in their repo.
The app already knows how to draw `brightness`. This is why Xiaomi's app
supports thousands of SKUs with one binary.

## Capability catalog (v1)

| Capability | Slots | Kind | R/W | App renders |
|---|---|---|---|---|
| `switch` | `power` | bool | rw | toggle |
| `brightness` | `level` | int 1–100 % | rw | slider |
| `color_temp` | `kelvin` | int 2000–6500 K | rw | warm/cool slider |
| `color_rgb` | `rgb` | string `#RRGGBB` | rw | colour wheel |
| `cover.position` | `position` | int 0–100 % | rw | vertical slider |
| `cover.control` | `command` | enum `open,close,stop` | w | three buttons |
| `fan.speed` | `level` | int 0–100 % | rw | segmented |
| `fan.mode` | `mode` | enum `auto,manual,sleep` | rw | segmented |
| `thermostat.mode` | `mode` | enum `off,heat,cool,auto` | rw | mode chips |
| `thermostat.target` | `target` | float °C 5–35 | rw | dial |
| `thermostat.current` | `value` | float °C | r | readout |
| `lock` | `locked` | bool | rw | lock button |
| `sensor.temperature` | `value` | float °C | r | tile |
| `sensor.humidity` | `value` | float % | r | tile |
| `sensor.contact` | `open` | bool | r | tile |
| `sensor.motion` | `detected` | bool | r | tile |
| `sensor.battery` | `level` | int 0–100 % | r | badge |
| `energy.power` | `watts` | float W | r | sparkline |
| `energy.total` | `kwh` | float kWh | r | number |
| `media.playback` | `state` | enum `play,pause,stop` | rw | transport |
| `media.volume` | `level` | int 0–100 % | rw | slider |
| `camera.stream` | `url` | string | r | player |
| `diagnostics.rssi` | `value` | int dBm | r | hidden |

Planned: `alarm.*`, `air_quality.*`, `irrigation.*`, `doorbell.*`. Adding one is
a platform release, not a vendor action.

### Capability definition (source of truth)

`protocol/src/capability.rs` holds the catalog; the JSON below is what the portal
serves to the app so both sides agree:

```json
{
  "name": "brightness",
  "version": 1,
  "slots": [
    { "slot": "level", "kind": "int", "access": "rw", "unit": "%",
      "min": 1, "max": 100, "history": "change" }
  ],
  "ui": { "widget": "slider", "icon": "sun", "group": "light" },
  "automation": { "compare": ["eq","gt","lt","gte","lte"], "set": true }
}
```

`history` is `off | change | interval:{secs} | all`. It controls whether telemetry
is written at all — a chatty power sensor at 1 Hz would otherwise dominate the
hypertable.

## Product definition

```json
{
  "schema_version": 1,
  "product_key": "hv_acme_bulb1",
  "name": "Acme Bulb E27",
  "category": "light",
  "icon": "bulb",
  "capabilities": [
    { "name": "switch", "points": { "power": { "dp": "1", "kind": "bool" } } },
    { "name": "brightness",
      "points": { "level": { "dp": "2", "kind": "int", "scale": 1, "min": 1, "max": 100 } } },
    { "name": "color_temp",
      "points": { "kelvin": { "dp": "3", "kind": "int", "min": 2000, "max": 6500 } } },
    { "name": "diagnostics.rssi",
      "points": { "value": { "dp": "101", "kind": "int" } } }
  ],
  "constraints": { "command_timeout_ms": 10000, "min_firmware": "1.0.0" }
}
```

### Point mapping rules

| Field | Meaning |
|---|---|
| `dp` | vendor's datapoint id, string, opaque to us |
| `kind` | `bool`, `int`, `float`, `string`, `enum`, `raw` |
| `scale` | multiplier applied vendor→platform (vendor sends 0–1000, we show 0–100 with `scale` 0.1) |
| `enum_map` | vendor value → platform enum, e.g. `{"1":"heat","2":"cool"}` |
| `min`/`max` | clamp; a report outside is rejected and logged, not stored |
| `access` | `r`, `w`, `rw` — the app hides writes it cannot do |

A write to a point the product does not declare is rejected at the API with
`422 unknown_point`; the device does not get to invent capabilities at runtime.

## Validation the portal enforces before publish

1. Every capability exists in the catalog at a known version.
2. Every slot the capability requires is mapped, with a matching `kind`.
3. No two capabilities claim the same `dp` with different kinds.
4. `enum` points carry an `enum_map` covering every platform enum value.
5. Ranges intersect the catalog range (a bulb cannot claim 0–10000 %).
6. If the product has `category: light`, `switch` is present — otherwise the app
   has no primary control and pairing looks broken.

A schema that fails is rejected with the exact rule number above; vendors see it
in the portal and in `POST /v1/products/{id}/validate` from CI.

## Versioning

`schema_version` is the product's, not the capability's. Publishing a change
that removes a capability, changes a point's `kind`, or narrows a range is a
**breaking** change and the portal requires a new `product_key` (`..._v2`).
Devices already shipped keep the old product row forever — a firmware update
points them at the new key, one device at a time.

Non-breaking (allowed in place, bumps `schema_version`): adding a capability,
widening a range, adding an optional point, changing `history`.

## Android/iOS state model

Each point row carries its current value denormalised (`value_num`,
`value_bool`, `value_text`) so a home's dashboard is one query, not N+1 across a
hypertable. History lives in `telemetry`; the current value never reads from it.

```
device_points
  id | device_id | capability | slot | kind | value_num | value_bool | value_text | updated_at
```

One row per (capability, slot). `code` in the API is `<capability>.<slot>`, e.g.
`brightness.level` — stable, readable, and what the automation DSL references.

## Standard categories

`light`, `plug`, `switch`, `sensor`, `cover`, `thermostat`, `lock`, `camera`,
`fan`, `purifier`, `vacuum`, `irrigation`, `appliance`.

Category is a UI grouping hint plus the product-validation defaults above. It is
not a behaviour; behaviour comes from capabilities.
