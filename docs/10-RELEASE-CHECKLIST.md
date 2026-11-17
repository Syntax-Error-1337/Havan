# Release checklist

What has to be true before this is sold, and who can check it. Everything in the first
table runs on this machine today. The three sections after it need hardware this machine
does not have — they are written as runnable steps so the first person with an Xcode Mac
and an ESP32 finishes them in an order, instead of re-deriving what was meant.

## Gates that run now

| Gate | Command | Green means |
|---|---|---|
| Build and tests | `make check` | fmt, clippy `-D warnings`, no secrets in the index, every route's path parameters match its handler, the whole suite as `haven_app` on `haven_test` (52 binaries) |
| Customer journey | `make e2e` | 34 assertions: register → home → ticket → forged ticket refused → claim → replay refused → command → history → another account cannot read or command, over HTTP |
| Licences | `make licences` | 365 crates, 1 035 node packages, none outside the allowlist (ADR-002) |
| Read performance | `make bench` | timings against `docs/08-PERFORMANCE.md` |
| Ingest | `make load RATE=600 SECONDS=20` | zero loss at whatever the host's generator reaches (~330/s single, 522/s with two); the loss column is the assertion, not the rate |
| Degradation | `make chaos` | 20/20: `503` not `500`, no driver text, recovery without a restart, refused commands redelivered, `/ops/health` refuses rather than showing an empty platform |
| App | `cd app && npx tsc --noEmit && npm test` | 100 tests, 19 suites (the script passes `--forceExit`; see the note below) |
| Palette contrast | `make contrast` (inside `make check`) | every text pair ≥ 4.5:1, every glyph and category accent ≥ 3:1, in both appearances |
| Portal unit | `cd portal && npx tsc --noEmit && npm test` | 63 tests |
| Portal end-to-end | `cd portal && npm run test:e2e` (+ `:phone`) | 40 tests per project (80 total) against a real API: sign-in, products, fleet, money, team, integrations, security, the operator console, 420 px layout |
| Vendor surfaces | `make vendor-smoke` | 25 assertions live: two-factor enrol → confirm → enforced sign-in → disabled, webhook shown once then removed, SSO refused while nobody is bound, an empty invoice period that is a document and not an error |
| Conformance | `make conformance VENDOR=<uuid> PRODUCT=<uuid>` | 7/7: a real device claims, connects, reports, is commanded, acks, reconnects, and an unknown serial is handled the way the product declares |

Two of these are worth running twice: `make chaos` because it stops containers, and
`make load` because a first run after a long idle can time out on a cold cache.

**The app suite leaks timers.** Run plain, `jest` finishes its assertions and then reports seven
suites failed because a worker would not exit; the failures are teardown, not assertions. `npm test`
therefore passes `--forceExit`, which makes the exit code mean what it looks like. That is a
papering-over, not a fix: the leak is React Query's timers outliving the test, and the honest repair
is unmounting in `afterEach` — worth doing, not worth blocking a release on.

**A one-second wait is a test that measures the machine.** `findBy*` gives up after 1 s by default,
which is ample on an idle laptop and not ample with several jest workers competing for a CPU. That
surfaced as an intermittent "cannot find Lamp" in the dashboard suite — a *test* artefact that reads
exactly like a broken screen, and the kind of failure people learn to re-run instead of read. The suite
now configures `asyncUtilTimeout: 5_000` once, in `src/test/setup.ts`; a genuinely missing element
still fails, and fails for the right reason.

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

## 3. Multi-replica — done, workers and API instances

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
