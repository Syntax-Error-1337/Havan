# Release checklist

What has to be true before this is sold, and who can check it. Everything in the first
table runs on this machine today. The three sections after it need hardware this machine
does not have — they are written as runnable steps so the first person with an Xcode Mac
and an ESP32 finishes them in an order, instead of re-deriving what was meant.

## Gates that run now

| Gate | Command | Green means |
|---|---|---|
| Build and tests | `make check` | fmt, clippy `-D warnings`, no secrets in the index, every route's path parameters match its handler, every client-facing timestamp is RFC 3339 (`make time-serde`), the app's colours all come from `tokens.ts` (`make tokens`), the palette's contrast (`make contrast`), the whole suite as `haven_app` on `haven_test` (53 binaries) |
| Customer journey | `make e2e` | 34 assertions: register → home → ticket → forged ticket refused → claim → replay refused → command → history → another account cannot read or command, over HTTP |
| Licences | `make licences` | 365 crates, 1 035 node packages, none outside the allowlist (ADR-002) |
| Read performance | `make bench` | timings against `docs/08-PERFORMANCE.md` |
| Ingest | `make load RATE=600 SECONDS=20` | zero loss at whatever the host's generator reaches (~330/s single, 522/s with two); the loss column is the assertion, not the rate |
| Degradation | `make chaos` | 20/20: `503` not `500`, no driver text, recovery without a restart, refused commands redelivered, `/ops/health` refuses rather than showing an empty platform |
| App | `cd app && npx tsc --noEmit && npm test` | 112 tests, 20 suites (the script passes `--forceExit`; see the note below) |
| Palette contrast | `make contrast` (inside `make check`) | every text pair ≥ 4.5:1, every glyph and category accent ≥ 3:1, in both appearances |
| Portal unit | `cd portal && npx tsc --noEmit && npm test` | 63 tests |
| Portal end-to-end | `cd portal && npm run test:e2e` (+ `:phone`) | 40 tests per project (80 total) against a real API: sign-in, products, fleet, money, team, integrations, security, the operator console, 420 px layout |
| Vendor surfaces | `make vendor-smoke` | 25 assertions live: two-factor enrol → confirm → enforced sign-in → disabled, webhook shown once then removed, SSO refused while nobody is bound, an empty invoice period that is a document and not an error |
| Conformance | `make conformance VENDOR=<uuid> PRODUCT=<uuid>` | 7/7: a real device claims, connects, reports, is commanded, acks, reconnects, and an unknown serial is handled the way the product declares |
| Contract | `make check` (inside it) | `scripts/route-arity.py` (route provides what its handler extracts), `scripts/portal-routes.py` (every `/vendor`, `/ops` and app `request(...)` path is a declared route), and `every_vendor_route_is_in_the_api_doc` — all three read the routers, none reads a list somebody maintains |
| Config documented | `cargo test -p kernel every_environment_variable` | every variable `kernel::Config` reads appears in `.env.example` — read from the source, so the key added last is the one caught. `HAVEN_CA_DIR` and `HAVEN_DB_POOL` were missing: both have defaults, which is why nobody noticed |
| Metrics documented | `crates/api/tests/metrics_contract.rs` | every `# TYPE` family the scrape emits appears in the table in `docs/05-API.md`. It found three stream gauges that had never been written down — a metric nobody documented is a metric nobody alerts on |
| Route coverage | `make check` (inside it) | `scripts/route-coverage.py --check` fails when a route is reached by nothing in the repository — no Rust test, no drill, no client, no broker configuration. `make coverage-map` prints the full table: 110 routes, all reached |
| No skipped tests | `make check` (inside it) | `scripts/no-skipped-tests.sh` runs the suite and fails if any test printed a skip. Seven tests were reporting "TEST_DATABASE_URL not set" with the database up — a row-level-security policy, a pool-level `set_config`, a missing owner row and a shared MQTT client id, all folded into the same `Option` and turned into green |
| Response headers | `cargo test -p api --test headers` | `nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer` and a `default-src 'none'` CSP on a success, on a `401`, and on both probes — and no `HSTS`, because this service does not terminate TLS |

`make verify` runs the live ones in the order they make sense — the customer journey, the vendor surface, both client
suites, and the fault drill last because it stops the containers. It is the release checklist as one command, and its
last run was **34 / 25 / 11 / 82 / 20, all green** on this tree.

Two of these are worth running twice: `make chaos` because it stops containers, and
`make load` because a first run after a long idle can time out on a cold cache.

**Not covered by any gate:** the portal is a single-page app, and a page needs a different policy from an API — it loads
its own script and style bundles, so `default-src 'none'` would break it. That policy belongs to whatever serves
`portal/dist`, and `deploy/` does not contain that host yet. Recorded rather than implied: a header the API cannot
honestly send is a deployment task, not a missing line.

### The five minutes are compilation, not tests

`make check` was described here as "the whole suite, 53 binaries, about five minutes", which invites the
obvious conclusion that the suites are slow and should be sharded. `scripts/check-timing.sh` times each gate:

| gate | time |
|---|---|
| testdb | 2.0 s |
| fmt --check | 0.5 s |
| no-secrets | 3.4 s |
| route-arity / contrast / tokens / time-serde | 0.7 s together |
| clippy --all-targets | 0.9 s warm, ~78 s cold |
| **cargo test --workspace** | **227 s**, of which **12.8 s is the suites running** |

The other ~170 seconds are `cargo test --workspace --no-run`: one change to `infra` relinks every test binary
that depends on it, and `api` has fifty. Two hypotheses were tested rather than assumed — that
`clippy --all-targets` evicts the test artefacts (it does not: `--no-run` after clippy is 1.2 s, and warm it is
0.3 s), and that sharding the suites would divide the time (there is nothing to divide).

So the fix is not a faster runner, it is not waiting for one: **`make check-fast`** runs fmt, secrets, the four
script gates and clippy without the database suites — the loop between edits. `make check` stays the gate
before a commit. Merging the fifty integration test files into a handful of binaries is the only change that
would move the number, and it would put the suites in parallel against one database, which is the interference
this workspace already has to avoid (the `scheduler` suites fire each other's automations). Correctness first:
the minutes stay, and the reason is written down so nobody spends an afternoon optimising four percent.


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
