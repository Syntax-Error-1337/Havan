# Release checklist

What has to be true before this is sold, and who can check it. Everything in the first
table runs on this machine today. The three sections after it need hardware this machine
does not have — they are written as runnable steps so the first person with an Xcode Mac
and an ESP32 finishes them in an order, instead of re-deriving what was meant.

## Gates that run now

| Gate | Command | Green means |
|---|---|---|
| Build and tests | `make check` | fmt, clippy `-D warnings`, the whole suite as `haven_app` on `haven_test` (30 binaries) |
| Customer journey | `make e2e` | register → home → ticket → forged ticket refused → claim → replay refused → command, over HTTP |
| Licences | `make licences` | 342 crates, 1 068 node packages, none outside the allowlist (ADR-002) |
| Read performance | `make bench` | timings against `docs/08-PERFORMANCE.md` |
| Ingest | `make load RATE=600 SECONDS=20` | ~600 msg/s with **no** loss; above that loss is expected and documented |
| Degradation | `make chaos` | 18/18: `503` not `500`, no driver text, recovery without a restart, refused commands redelivered |
| App | `cd app && npx tsc --noEmit && npm test` | 70 tests, 15 suites |
| Portal | `cd portal && npx tsc --noEmit && npm test` | 50 tests |

Two of these are worth running twice: `make chaos` because it stops containers, and
`make load` because a first run after a long idle can time out on a cold cache.

## 1. The phone pass — Phase 4's missing acceptance

Blocked on: a Mac with Xcode (`xcode-select --install` is not enough; this machine has
CommandLineTools, so `xcrun simctl` exits 72).

```bash
make app            # Metro on :8081
# press i for the simulator, or scan the QR from Expo Go on a phone
```

Then, on the simulator or device, in under 90 seconds:

1. Register an account, create a home.
2. Open **Add device**: the radar scans, the bulb appears, tap it.
3. Enter Wi-Fi credentials, watch the claim succeed.
4. The device appears on the dashboard; toggle it and watch the switch hold.

Pass means all four with no stalls, no blank screens, no retry. Record a screen capture —
the acceptance for this phase is a video, not a sentence.

Known-good on web (`make app-web`, :8082) for every step except the radar: BLE is mocked
there (`EXPO_PUBLIC_BLE_MOCK=1`), so the radar's empty and error states are exercised but
not a real scan.

## 2. Hardware SDK — Phase 7

Blocked on: an ESP32-S3 (or any board with an MQTT client and a toolchain).

Order matters — each step assumes the previous one is green:

1. **Port the reference client to C** against `docs/03-SDK-CONTRACT.md`. The Rust SDK
   (`backend/crates/haven-sdk`) is the reference: topics built not formatted, subscription
   re-made on every `ConnAck`, typed refusals, shadow carrying firmware.
2. **Pass checklist items 1–8** in `docs/03-SDK-CONTRACT.md` — claim with a ticket,
   publish a shadow, receive a command, ack it, refuse a bad value with a typed reason,
   report OTA progress, survive a broker restart, keep the shadow current across a reboot.
3. ~~`haven-cert` enrolment, broker mTLS, expiry alerts~~ — **done in software**: the CA and
   `POST /provision/certificate` issue a device certificate, the broker's 8883 listener requires
   one (`openssl s_client` verifies both the accept and the reject), and a six-hourly sweep
   warns at 30/14/7/0 days. What remains is a device actually holding the certificate.
4. **The portal badge**: a product is certified when its firmware passes the checklist;
   until then the store shows it as uncertified.

Reality check: no firmware agent exists in this repo. `/tmp/ota_success.py` and the
simulator *report* OTA progress; nothing actually flashes.

## 3. Multi-replica — done for workers, open for API instances

`make multi-replica` starts two workers and asserts what one worker cannot show:

1. **Exclusive delivery** — 480 messages land as exactly 960 rows. Verified.
2. **Redelivery is single-send** — one claim, one publish, `attempts = 2`. Verified, and it
   found a race: redelivery used to publish *then* mark, so both workers could publish one
   press. It now claims the row first and releases it if the publish fails.
3. **Health per replica** — `make api-replicas` starts two API instances: both report
   `db: false` and `503` when Postgres stops, agree with each other, recover without a restart,
   and a replica launched while the broker is down does not claim `mqtt: true`. Verified, and it
   caught a shared MQTT client id that made two replicas evict each other.
4. **Metrics do not double-count** — `/metrics` is per-process, so a fleet's numbers are the
   sum; a scrape config that assumes one instance will under-report by half.

## What is deliberately not on this list

- **A live push.** Delivery is implemented and exercised through the log transport; the APNs
  transport needs an Apple developer account, a `.p8` key and a real device token before
  "the phone buzzed" can be claimed. The queue semantics, the JWT and the request shape are
  tested.
- **Matter, voice, cameras, energy.** v2 in `docs/07-ROADMAP.md`.
- **A production deploy.** `deploy/docker-compose.yml` binds every port to loopback and
  runs a broker that allows anonymous connections; it is a development stack and says so.
