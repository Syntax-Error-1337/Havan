# ADR-004 — Using TuyaOpen as a reference for our device SDK

**Status:** accepted; steps 1–3 implemented, step 4 open
**Context:** Phase 7 is "reference implementation done, certification checklist pending". The gap is
not protocol — `sdk/c` is a 250-line codec with 45 checks — it is everything a vendor needs to *ship*
a device: a porting boundary, provisioning, OTA verification, storage, watchdog. TuyaOpen is a mature
open-source SDK that solves exactly those problems for Tuya's cloud.

## Licence check (AGENTS.md §1)

TuyaOpen is **Apache-2.0** (`LICENSE`, "Copyright (c) Tuya.inc 2019"). Compatible with our rule. Its
presence in a tree we host is not a licence event.

## The decision: reference, not dependency

**We cannot depend on it.** TuyaOpen is not a generic MQTT device SDK — it is a client for Tuya's
cloud. `tuya_iot.c` and `tuya_register_center.c` authenticate with a Tuya-issued token and device
key, `iotdns.c` resolves Tuya endpoints, and `mqtt_service.c` speaks Tuya's topic and payload
contract. Adopting it would mean our devices talk to Tuya, with Haven on the side — the opposite of
the product, and a permanent dependency on a competitor's control plane.

**We should copy its shape.** The parts worth having are architectural, and they are the parts we are
missing:

| TuyaOpen | What it solves | What we have |
|---|---|---|
| `tal_*` (abstraction) over `tkl_*` (porting) | one SDK, many chips: ESP32, BK7231X, GD32, JIELI, L511x | nothing — the ESP8266 firmware calls BearSSL and PubSubClient directly |
| `boards/` + Kconfig + CMake | a vendor adds a board without touching the SDK | one hand-written `pio` project |
| `netcfg/` (AP + BLE provisioning) | getting Wi-Fi credentials onto a device in the field | in the firmware, plus an app-side radar; no shared implementation |
| `tuya_ota.c` | SHA-256 over the image, HMAC with the device key, resume, report | `ota` topics and a report helper; no verification, no rollback |
| `tuya_health.c` | watchdog, periodic self-checks | none |
| `tuya_cert_manager.c` | certificate lifecycle and storage | certificates issued by us, stored by the firmware's own code |
| `examples/` | a vendor's starting point | one `emit.c` |

## What we adopt, in order

1. **A porting boundary: `haven_port.h`.** Four function groups — `net` (connect/send/close), `tls`
   (handshake with a client cert), `store` (get/set a blob), `sys` (time, reboot, entropy). No POSIX
   calls in the protocol code, and a POSIX implementation used by the tests, so the boundary is
   exercised rather than declared.
2. **A `haven_runtime.c` that owns the connection.** Today the ESP8266 sketch implements connect,
   resubscribe, keepalive, will, backoff and re-auth by hand — the `tls.stop()` and `mqtt.state()`
   bugs both lived there. That logic belongs in the SDK, once, so a vendor cannot get it wrong.
3. **OTA verification, with our crypto rather than theirs.** TuyaOpen hashes the image and HMACs it
   with the device's secret. That proves the image came *from the cloud* — it does not stop a
   malicious cloud, and our vendors are the ones signing their own firmware. We use an Ed25519
   signature by the vendor key, verified against the public key pinned in the device at
   manufacture, plus a boot-time rollback flag. Same flow, stronger trust root.
4. **A conformance suite instead of a prose checklist.** Phase 7's blocker is that certification is a
   document. TuyaOpen's `examples/` show the shape: a program a vendor runs against a staging
   deployment that exercises claim, shadow, command apply and typed refusal, OTA, and reconnect —
   and prints pass/fail per item. That is what "certified" should mean.

## What we explicitly do not copy

- **Their protocol and auth**: Tuya PIDs, activation tokens, `iotdns`, `atop`. Adopting these is
  adopting Tuya's cloud.
- **Whole-file copies.** Licence-compatible, but it would import a 100 000-line codebase maintained
  against a different cloud, and every future upstream change would be a merge we did not ask for.
  Structure and ideas are borrowed; code is ours.
- **Their HMAC trust model** for OTA, per item 3.

## What landed

- **`sdk/c/haven_port.h`** — the boundary: `connect`/`disconnect`/`subscribe`/`publish`/
  `next_message`, plus `now_ms`, `random_u32`, `ota_verify`, `ota_install`, `reboot`. No POSIX, no
  sockets, no TLS: a vendor points these at their platform, a test points them at a fake.
- **`sdk/c/haven_runtime.c`** — the loop, once: connect with the device id and secret, subscribe
  per connection, last will on the shadow topic, exponential backoff with quarter-width jitter
  capped at 60 s, and OTA applied only after `ota_verify` approves the image.
- **`sdk/c/tests/test_runtime.c`** — nine checks against a fake port, no broker or board needed.
  Two of them are regressions: a reconnect must be preceded by a disconnect (the reused-TLS-client
  bug), and fifty ticks must produce one subscribe, not fifty.
- `make sdk` runs the 46 protocol checks and the nine runtime checks; the ESP-IDF component
  compiles both files.

**Not yet wired into a device.** The ESP8266 sketch still runs its own loop; adopting the runtime
there means implementing `haven_port` over `PubSubClient` + BearSSL, which is the next step and is
worth doing with the board attached, because that is the only way to prove the port end to end.

## Consequences

- The C SDK grows from a codec into a small runtime. It stays small: the target is roughly the size
  of the ESP8266 sketch it replaces, not the size of TuyaOpen.
- Certification becomes executable, which is the only version of "certified" that a vendor cannot
  argue with.
- We keep one protocol implementation (`sdk/c`, `sdk/esp8266`) and a Rust one for the simulator, so
  the conformance suite tests the contract rather than a particular language binding.
