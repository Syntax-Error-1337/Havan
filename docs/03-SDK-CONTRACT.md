# SDK contract v1 — what a vendor builds against

Normative. `MUST` / `MUST NOT` are certification requirements; `SHOULD` is a
recommendation with a stated cost; `MAY` is optional. The platform accepts some
non-conforming input; that is not permission to rely on it.

Two implementations ship from this document: `sdk/c` (ESP-IDF component, the
reference) and `sdk/rust` (`haven-device` crate, for gateways and Linux SoCs).
`sdk/c/test/` and the simulator in `backend/crates/simulator` are the
conformance suite; `haven-cert` runs them against a deployment.

---

## 1. Identity

| Item | Rule |
|---|---|
| `serial` | 12–32 chars, `^[A-Z0-9-]+$`, unique per vendor, printed on the label and encoded in the QR |
| `device_secret` | 32 random bytes burned at manufacture, stored server-side as Argon2id hash, never leaves the device |
| `product_key` | assigned by the portal, `hv_<vendor>_<sku>` |
| Device certificate | issued at first claim from a CSR the device generates; CN = device id, chain under the vendor intermediate |
| Key storage | MUST be in secure storage (ESP32 NVS encrypted / ATECC608 / TPM). A secret in plain flash fails certification. |

The certificate is the operational credential; the secret is used exactly once
per device lifetime (first claim), plus once more if the device is factory-reset.

---

## 2. Pairing over BLE (the radar)

### 2.1 Advertising

While unprovisioned the device MUST advertise connectably with:

```
Flags: LE General Discoverable, BR/EDR not supported
Complete 128-bit Service UUID: 5a1d0001-8f2b-4c6e-9d1a-7c3e5f9b2a10
Service Data (same UUID): 4-byte product hash | 4-byte serial tail | 1-byte state
```

- **product hash** = first 4 bytes of `SHA-256(product_key)`. The app maps it to
  a name, icon and category from a catalog it caches, so the radar shows
  "Acme Bulb" and not a MAC address before any connection is made.
- **serial tail** = last 4 bytes of the serial, for disambiguation when a shelf
  has twenty identical boxes.
- **state** = `0x01` unprovisioned, `0x02` provisioning, `0x03` needs re-pair,
  `0x04` claimed but offline-entitled (already belongs to a home).

Advertising interval 100–300 ms while unprovisioned, then stop advertising
entirely once claimed. A claimed device MUST NOT be discoverable by the radar.

### 2.2 GATT

| Handle | UUID | Properties | Payload |
|---|---|---|---|
| Info | `5a1d0002-…` | Read | UTF-8 JSON, ≤ 512 B |
| Rx (app→device) | `5a1d0003-…` | Write Without Response | framed |
| Tx (device→app) | `5a1d0004-…` | Notify | framed |

Info:

```json
{"pk":"hv_acme_bulb1","sn":"ACME-0001-8F2C","fw":"1.2.0","hw":"b1","proto":1,"mtu":185}
```

Framing on Rx/Tx — because a JSON message can exceed one ATT payload:

```
byte 0      frame type: 0x01 start, 0x02 continuation, 0x03 last, 0x04 single
byte 1..2   message id (u16 BE), same for every chunk of one message
byte 3..4   total length (u16 BE), present on start/single only
byte 5..    chunk
```

A device MUST reassemble up to **2048 bytes** and MUST drop a message that is
not complete within 10 s. Negotiate MTU ≥ 185 where the stack allows; a device
MUST work at the default 23 (20-byte payload) — it is slower, not broken.

### 2.3 Exchange

```
app → dev  {"t":"hello","proto":1,"app":"1.0.0","lang":"en"}
dev → app  {"t":"ready","pk":"…","sn":"…","fw":"1.2.0","caps":["wifi"]}
app → dev  {"t":"scan"}                       // optional, triggers a Wi-Fi scan
dev → app  {"t":"networks","list":[{"ssid":"Home","rssi":-52,"auth":"wpa2"}]}
app → dev  {"t":"provision",
            "ssid":"Home","pass":"…","auth":"wpa2",
            "ticket":"<opaque, ≤1024 B>",
            "endpoint":"https://api.haven.dev",
            "tz":"Asia/Kolkata","lang":"en"}
dev → app  {"t":"joining"}                    // app switches to "connecting" UI
dev → app  {"t":"result","ok":true,"device_id":"…"}   // after cloud claim
           {"t":"result","ok":false,"code":"wifi_auth","msg":"wrong password"}
```

Rules:

- The device MUST refuse `provision` while claimed, unless the user held the
  physical reset gesture — otherwise a neighbour re-pairs your bulb by BLE.
- The ticket is opaque and MUST be forwarded verbatim; the device MUST NOT try
  to parse or cache it beyond the claim attempt.
- `pass` MUST be accepted in WPA2-PSK and WPA3-SAE; open networks are sent with
  `"auth":"open"` and an empty password.
- The device SHOULD emit progress (`joining`, `dhcp`, `claiming`) so the app can
  show which step failed. A device that reports only success/failure is
  certified but unpleasant.
- On `result.ok:false` the device MUST return to advertising within 5 s.

### 2.4 Fallbacks

| Mode | When | How |
|---|---|---|
| AP / SoftAP | BLE unavailable on the phone, or BLE pairing failed twice | device raises `Haven-<serial-tail>` AP; app posts the same `provision` JSON to `http://192.168.4.1/provision` |
| QR | bulk install | label QR = `haven://p?pk=…&sn=…&c=<claim code>`; app skips scan, goes straight to Wi-Fi |
| Matter | multi-ecosystem SKUs | commissioning via Matter, then the device registers itself with Haven using a Matter-derived token |

All three converge on the same `provision` payload and the same claim endpoint.
That is deliberate: one server path to test.

---

## 3. First claim (bootstrap)

```
POST https://api.haven.dev/v1/provision/claim
Content-Type: application/json

{ "sn":"ACME-0001-8F2C", "pk":"hv_acme_bulb1", "secret":"<hex 64>",
  "ticket":"<verbatim>", "csr":"<base64 DER PKCS#10>",
  "fw":"1.2.0", "hw":"b1", "mac":"AA:BB:CC:DD:EE:FF", "ts":1699999999 }
```

Response `201`:

```json
{ "device_id":"d_01H…",
  "cert":"-----BEGIN CERTIFICATE-----\n…",
  "chain":["-----BEGIN CERTIFICATE-----\n…"],
  "mqtt":{"host":"mqtts://mqtt.haven.dev","port":8883,"client_id":"d_01H…","keepalive":30},
  "home":{"id":"h_01H…","name":"Home"},
  "points":[{"code":"switch.power","dp":"1","kind":"bool"}] }
```

Errors are stable and actionable:

| HTTP | `code` | Device action |
|---|---|---|
| 400 | `ticket_expired` | return to BLE advertising; app mints a new ticket |
| 401 | `bad_secret` | log, blink red, do not retry more than 3 times |
| 404 | `unknown_serial` | device is not in the vendor's manifest — manufacturing error |
| 409 | `already_claimed` | factory-reset path only |
| 422 | `ticket_home_mismatch` | app bug; abort |
| 429 | `rate_limited` | back off 30 s, retry once |
| 503 | `overloaded` | retry with jitter, 5 attempts over 60 s |

The claim MUST be idempotent for 10 minutes on `(sn, ticket)`: a flaky Wi-Fi
link retrying must not create a second device. The claim is refused over plain
HTTP — the SDK MUST pin the platform CA and MUST NOT accept a redirect.

After a successful claim the device persists `device_id`, cert, key and MQTT
config, deletes the ticket, and stops advertising.

---

## 4. MQTT

### 4.1 Connection

| Parameter | Value |
|---|---|
| Broker | `mqtts://<host>:8883`, TLS 1.3, **mutual** TLS |
| Client id | the device id; `^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$` |
| Auth | the certificate is the credential; no password |
| Protocol | MQTT **5.0** |
| Keep-alive | 30 s (server declares dead at 45 s, then fires the Will) |
| Clean start | `false`; session expiry 1 h, so commands queued during a short outage arrive |
| QoS | 1 on every publish and every subscription |
| In-flight | ≤ 16 unacknowledged |
| Will | retained, on `shadow`: `{"v":1,"online":false,"ts":…}` |

### 4.2 Topics

Root `haven`. Never hand-format these; use `protocol::topics`.

| Topic | Direction | Retained | Purpose |
|---|---|---|---|
| `haven/v1/{vendor}/{home}/{device}/evt` | dev → cloud | no | state and telemetry reports |
| `haven/v1/{vendor}/{home}/{device}/shadow` | dev → cloud | **yes** | full state + online flag; LWT target |
| `haven/v1/{vendor}/{home}/{device}/cmd` | cloud → dev | no | property writes, reads, actions |
| `haven/v1/{vendor}/{home}/{device}/ack` | dev → cloud | no | settles one command id |
| `haven/v1/{vendor}/{home}/{device}/ota` | both | no | firmware manifest down, progress up |

The ACL grants a device exactly its own lane. A device that subscribes to
another device's topic gets SUBACK `0x87` from the broker, not a client-side
courtesy check.

### 4.3 Payloads

All are compact JSON, `v` first, unknown fields ignored (forward compatibility),
`ts` in epoch **seconds**.

**Event** — one or more changed datapoints:

```json
{"v":1,"ts":1699999999,"seq":42,"pts":{"1":true,"2":80}}
{"v":1,"ts":1699999999,"seq":43,"pts":{"15":23.5},"batt":87}
```

`seq` is a monotonically increasing u32 per device, reset only on factory reset;
it lets ingest detect loss and reordering in logs without trusting the broker.
A report carrying a datapoint the product does not declare is logged and dropped
(never stored, never fanned out).

**Shadow** — published retained immediately after SUBACK, and again after any
reconnect, and whenever more than 10 % of points changed:

```json
{"v":1,"ts":1699999999,"online":true,"fw":"1.2.0","rssi":-61,
 "pts":{"1":true,"2":80,"15":23.5}}
```

**Command**:

```json
{"v":1,"id":"9f1c…","ts":1699999999,"op":"set","dp":2,"value":80}
{"v":1,"id":"9f1c…","ts":1699999999,"op":"get","dp":15}
{"v":1,"id":"9f1c…","ts":1699999999,"op":"action","action":"identify"}
```

`op` is `set | get | action`. `set` MUST be applied or refused within 5 s.

**Ack** — exactly one per command id, exactly once:

```json
{"v":1,"id":"9f1c…","ts":1699999999,"ok":true,"dp":2,"value":80}
{"v":1,"id":"9f1c…","ts":1699999999,"ok":false,"code":"invalid_value",
 "msg":"brightness must be 1..100"}
```

Stable codes: `invalid_value`, `unsupported_point`, `busy`, `unauthorized`,
`not_ready`, `hardware_fault`, `unsupported_op`. `msg` is for humans and may
change; `code` may not.

A device MUST NOT ack a command it has not applied. Optimistic acks make the
app lie, and a lying app is worse than a slow one.

### 4.4 Ordering

Messages from one device are folded in arrival order, and ingest shards by
device id so one device's reports always reach the same writer. Ordering is
**not** promised across a reconnect: the broker's queued copy and the live copy
can interleave, which is why `seq` exists.

---

## 5. OTA

Manifest, published to `ota` or fetched from `GET /v1/ota/manifest`:

```json
{"v":1,"version":"1.2.0","url":"https://cdn.haven.dev/fw/…bin",
 "size":812344,"sha256":"…","sig":"<base64 Ed25519 over sha256>",
 "mandatory":false,"rollout_id":"…"}
```

- The device MUST verify `sha256` **and** the Ed25519 signature against the
  vendor public key burned into the bootloader. A `200` from the CDN is not
  authenticity.
- The device MUST write to the inactive partition and MUST keep the running one
  bootable until the new image has run 60 s and published `state:"success"`.
- Progress reports, published by the device on its own `ota` topic:
  `{"v":1,"campaign_id":"…","state":"downloading|verified|rebooting|success|failed|skipped","pct":42,"code":"…"}`.
  `rollout_id` is accepted as a synonym for `campaign_id`. `pct` is optional and
  only meaningful while downloading. The cloud ignores `pending`/`sent` from a
  device (those are its own states) and ignores anything after `success`.
- A device that fails to boot twice MUST self-rollback and report
  `failed` with `code:"boot_failed"`.
- The platform halts a campaign when failure exceeds 2 % of the cohort.

---

## 6. C API surface (ESP-IDF component)

```c
#include "haven.h"

typedef struct {
    const char *product_key;
    const char *serial;
    const uint8_t secret[32];
    const char *broker_host;      // used after claim
    const char *ca_pem;           // platform CA, pinned
    haven_log_fn log;
} haven_config_t;

esp_err_t haven_init(const haven_config_t *cfg);
esp_err_t haven_start(void);                        // claim if needed, then connect MQTT

// called from the cloud when a command arrives; return ESP_OK once applied
typedef esp_err_t (*haven_command_fn)(const haven_command_t *cmd, haven_value_t *reported);
void haven_set_command_handler(haven_command_fn fn);

esp_err_t haven_report(const char *dp, haven_value_t v);        // coalesced, ≤ 1 Hz per dp
esp_err_t haven_report_batch(const haven_dp_pair_t *pairs, size_t n);
esp_err_t haven_shadow_flush(void);                             // retained full state
esp_err_t haven_ack(const char *cmd_id, bool ok, const char *code);

esp_err_t haven_ble_start(haven_ble_state_t state);             // advertising on/off
esp_err_t haven_factory_reset(void);                            // wipes cert, re-advertises
```

Rules the SDK enforces so vendors cannot get them wrong:

- `haven_report` coalesces per datapoint (latest wins) and flushes on a 1 s tick
  or 20 pending points — a firmware loop cannot flood the broker.
- Reconnect uses jittered exponential backoff 1 s → 60 s, forever, and republishes
  the shadow on every reconnect.
- The MQTT task has a 6 KB stack budget and no `malloc` after `haven_start`.
- `haven_init` fails loudly if `secret` is all zeroes or `ca_pem` is missing.

---

## 7. Certification checklist

`haven-cert` (in `backend/crates/cert`) runs these against a firmware image or
the simulator and produces a signed report the portal shows as a badge:

1. BLE advertises the right service data for the product; stops after claim.
2. Full provisioning exchange under 90 s, and under 120 s at MTU 23.
3. Claim is idempotent across a retry; a replayed ticket is refused.
4. `set` for every writable point acks within 5 s with the value echoed.
5. An out-of-range `set` is refused with `invalid_value`, not clamped silently.
6. Shadow republishes after a forced reconnect.
7. LWT fires within 45 s of a power cut (simulated by dropping the socket).
8. OTA rejects a tampered image (flipped byte) with `verify_failed`.
9. OTA rolls back from an image that panics on boot.
10. `seq` increases monotonically across a 10-minute soak.

A product cannot be published until its reference firmware passes 1–8; 9–10 are
required for marketplace listing.

### Re-subscribe after every reconnect (MUST)

Subscriptions belong to the MQTT session. A device that asks for a clean session
comes back **deaf** after a reconnect, and nothing in the protocol says so — the
broker accepts the connection, the platform sees a connected device, and every
command times out silently.

Subscribe on `CONNACK`, not only at startup. Idempotent, so the first connect does
it twice and nothing breaks.

This is not hypothetical: the reference simulator had exactly this bug. It worked
until the broker restarted, then never heard another command for the rest of its
life. Proven fixed by restarting VerneMQ and sending a command afterwards.

### Report the result even when it worked (MUST)

`success` is not decoration. It is the only signal that changes what the platform
believes the device runs: the cloud writes the campaign's firmware version onto the
device record when it arrives, and until then the customer's app cannot say "up to
date" and the vendor's rollout table cannot close.

Report `downloading` → `verified` → `rebooting` → `success`, with `pct` while
downloading. A device that flashes silently leaves a rollout running forever and a
customer reading a version that is no longer true.

The offer envelope accepts `campaign_id` or `rollout_id`; the report likewise.

