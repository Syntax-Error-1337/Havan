# Architecture

## Shape

```
                        ┌──────────────── vendor portal (React) ─────────────┐
                        │                                                    │
 device (SDK)           │                                                    │
   │  MQTT 5 / mTLS    ┌▼────────────┐   ┌──────────────┐   ┌───────────────┐│
   ├──────────────────►│  EMQX       │──►│ worker       │──►│ Postgres      ││
   │                   │  (broker)   │   │  ingest      │   │  + Timescale  ││
   │◄──────────────────┤             │   │  scheduler   │◄──│               ││
   │  commands         └─────────────┘   │  ota         │   └───────────────┘│
   │                                     └──────┬───────┘                    │
   │                                            │ events/commands           │
   │   BLE (pairing only)                       ▼                            │
 customer app ──HTTPS/WS──► ┌──────────────────────────┐ ◄──HTTPS────────────┘
                            │  api (axum)              │
                            │  auth · homes · devices  │──► Redis (presence, rate, sessions)
                            └──────────────────────────┘
```

Two processes, one codebase. `api` serves humans; `worker` owns everything that
happens without a request: MQTT ingest, automation evaluation, OTA campaigns,
notification fan-out, retention. They share `domain` and `infra` and never talk
to each other directly — Postgres `LISTEN/NOTIFY` and Redis streams are the bus.

## Why two binaries and not one

| | single binary | api + worker |
|---|---|---|
| Deploy granularity | all or nothing | scale ingest independently of reads |
| Blast radius | an ingest bug takes down logins | separate |
| Migrations | one owner | one owner (`api` runs them) |
| Local dev | simpler | `make api worker` (one extra terminal) |

An ingest bug should not be able to take down login. Two binaries.

## Rust workspace

```
backend/
  Cargo.toml                 # workspace, [workspace.dependencies]
  crates/
    protocol/                # wire types: topics, envelopes, thing model, BLE frames
    kernel/                  # config, error, ids, clock, crypto, tenancy context, telemetry
    domain/                  # feature slices (see below) — pure logic + traits
    infra/                   # postgres, redis, mqtt, object store, push
    api/                     # axum: routers, middlewares, OpenAPI  (bin: haven-api)
    worker/                  # ingest, scheduler, ota, notify        (bin: haven-worker)
    migrate/                 # sqlx migrations runner                (bin: haven-migrate)
    simulator/               # virtual fleet for CI + load           (bin: haven-sim)
```

Dependency rule, enforced by `cargo-deny` + a test that greps `Cargo.toml`:

```
protocol  ◄── kernel ◄── domain ◄── infra ◄── api | worker
                                 └──────────┘
```

- `protocol` depends on nothing but serde/uuid/time.
- `kernel` may not depend on `sqlx`, `axum`, `rumqttc`.
- `domain` may not depend on `axum` or `rumqttc` — it defines traits (`DeviceRepo`,
  `CommandPublisher`) that `infra` implements. This is what makes the rule engine
  testable with no database and no broker.
- `api`/`worker` are the only crates that know about both.

## Feature slices in `domain`

Vertical, not layered. Each folder owns its router-agnostic use cases, its
entities, its repository trait, and its errors.

```
domain/src/
  identity/      users, sessions, refresh rotation, password policy
  vendors/       tenants, vendor users, quotas, white-label config
  catalog/       products, thing models, capability templates, firmware metadata
  homes/         homes, members, invitations, rooms
  devices/       devices, points, shadow, command dispatch, state history
  pairing/       claim tickets, provisioning sessions, BLE handshake state
  automation/    triggers, conditions, actions, evaluator, scheduler
  ota/           campaigns, rollout math, device jobs
  notify/        push endpoints, templates, fan-out rules
  audit/         append-only actor/action log
```

Every slice has the same five files:

```
devices/
  mod.rs        // public API of the slice: commands + queries
  model.rs      // entities, value objects (no serde-on-DB leakage)
  repo.rs       // trait DeviceRepo { ... }
  service.rs    // use cases; takes &impl DeviceRepo
  error.rs      // thiserror enum, mapped once in api/error.rs
```

No slice may import another slice's `repo` or `model` directly — only its
`mod.rs` surface. Cross-slice coordination (e.g. pairing creating a device) goes
through an explicit port trait, and the composition root wires it.

## Data model

Postgres 16 + TimescaleDB. Multi-tenant with RLS, one `vendor_id` on every
tenant-owned row.

```
vendors(id, slug, name, status, quota_devices, created_at)
vendor_users(id, vendor_id, email, role, password_hash, totp_secret)
api_keys(id, vendor_id, name, prefix, hash, scopes, last_used_at)

products(id, vendor_id, product_key UNIQUE, name, category, thing_model JSONB,
         schema_version, status, image_url)
product_capabilities(product_id, capability, version)     -- derived from thing_model
firmware(id, product_id, version, size, sha256, ed25519_sig, url, status, rollout_pct)

users(id, email, phone, password_hash, display_name, locale, created_at)
homes(id, owner_id, name, timezone, geo, created_at)
home_members(home_id, user_id, role)                      -- owner|admin|member|guest
rooms(id, home_id, name, icon, sort)

devices(id, vendor_id, product_id, home_id, room_id, name, serial, cert_serial,
        secret_hash, fw_version, online, last_seen_at, transport, ip,
        claimed_at, state, metadata JSONB)
device_points(id, device_id, capability, code, kind, writable, unit, min, max,
              value_num, value_bool, value_text, updated_at)   -- current value
telemetry(device_id, point_id, ts, num, bool, text)             -- hypertable

commands(id, vendor_id, device_id, point_id, op, payload JSONB, desired JSONB,
         reported JSONB, status, idempotency_key, requested_by,
         requested_at, dispatched_at, acked_at, error_code, dispatch_path)
pairing_tickets(id, home_id, user_id, nonce_hash, expires_at, consumed_at)
provisioning_sessions(id, ticket_id, device_serial, transport, stage, created_at)

automations(id, home_id, name, enabled, definition JSONB, next_run_at)
automation_runs(id, automation_id, trigger_kind, started_at, finished_at, status, log)

ota_campaigns(id, product_id, firmware_id, rollout_pct, status, created_at)
ota_jobs(campaign_id, device_id, state, attempts, updated_at)

notifications(id, user_id, kind, payload JSONB, read_at, created_at)
push_tokens(id, user_id, platform, token, app_version)
audit_log(id, vendor_id, actor, action, subject, ip, meta JSONB, ts)
mqtt_acl(vendor_id, device_id, home_id)         -- read by EMQX authz hook, cached
```

**RLS is the tenant boundary, not a WHERE clause.** Every tenant table gets:

```sql
CREATE POLICY tenant_isolation ON devices
  USING (vendor_id = current_setting('haven.vendor_id')::uuid);
```

The pool sets `SET LOCAL haven.vendor_id` per transaction from the authenticated
principal. A forgotten filter then returns zero rows instead of another vendor's
fleet. Worker paths that must cross tenants use a `haven_system` role that
bypasses RLS, and only `domain` code holding a `SystemScope` token may use it.

**Hot state is not in Postgres.** Presence, last-seen, command dedupe and rate
limits live in Redis with TTLs; Postgres keeps the durable row. A device that is
offline has `online=false` in Redis after 90 s and in Postgres after the next
write-back — reads prefer Redis.

## MQTT contract

EMQX 5, MQTT 5, TLS 1.3, client cert auth (`peer_cert_as_username`), per-device
ACL generated from `mqtt_acl`. Topic root `haven`, exactly as documented in
[03-SDK-CONTRACT.md](03-SDK-CONTRACT.md):

```
haven/v1/{vendor}/{home}/{device}/evt        device → cloud   (unretained)
haven/v1/{vendor}/{home}/{device}/shadow     device → cloud   (retained, LWT target)
haven/v1/{vendor}/{home}/{device}/cmd        cloud  → device
haven/v1/{vendor}/{home}/{device}/ack        device → cloud
haven/v1/{vendor}/{home}/{device}/ota        cloud  → device
```

`vendor` is in the topic because the broker ACL is per-tenant and a shared
subscription (`$share/ingest/haven/v1/+/+/+/evt`) then needs no per-vendor rule
rewrite. Identity always comes from the topic and the TLS certificate, never
from the payload.

`worker` subscribes with a shared subscription across N instances; sharding is
by `device_id` hash inside the process so one device's reports always hit the
same writer, which is what makes ordering work without a distributed lock.

## Command path (the one that must not be wrong)

```
POST /v1/devices/{id}/commands
  │ 1. authz: caller is a member of the home, role >= member
  │ 2. validate: point exists, writable, value matches kind+range
  │ 3. idempotency: (device, idempotency_key) → existing command? return it
  │ 4. persist: INSERT command (status=queued, desired=value)
  │ 5. shadow: write desired into Redis, publish cmd (QoS 1)
  │ 6. respond 202 {command_id}
  ▼
device applies → publishes ack {id, ok, reported}
  ▼
worker: match command_id, UPDATE reported/status, publish point update,
        emit WS event to every open session of every member
```

Failure is explicit, never silent: `timeout` at 10 s (configurable per product),
`rejected` when the device says `ok:false` with a stable `error_code`,
`unreachable` when presence says offline before dispatch.

LAN fast path is a later phase and is a *carrier*, not a second control plane:
same validation, same command row, same shadow write, only the transport changes.

## WebSocket (live app state)

One socket per app session at `/v1/stream`, authenticated with the access token
in the first frame (not the query string — it lands in logs). Server messages
are `{t, id, data}`; the client subscribes to `home:{id}` topics on connect.

Events: `point.changed`, `device.online`, `device.offline`, `device.added`,
`device.removed`, `command.settled`, `automation.fired`, `ota.progress`.

Fan-out uses Redis pub/sub so any `api` instance can serve any socket; the
publishing `worker` does not need a connection to every API pod.

## Scale envelope (v1 target)

| Dimension | Target | Mechanism |
|---|---|---|
| Devices | 1 M | sharded ingest, Timescale compression |
| Events/s sustained | 20 k | batching (500 ms or 500 rows, whichever first), COPY |
| Concurrent app sockets | 100 k | Redis fan-out, `api` horizontal |
| Command p95 | 400 ms | single publish, no DB round-trip on the hot path |
| Tenants | 5 k | RLS, per-vendor token bucket |

Ingest writes go through a batching writer per shard; one transaction per batch,
`COPY` into `telemetry`. Backpressure is a bounded channel — when it is full we
shed telemetry before we shed commands, because losing a sample is cheap and
losing a command is not.

## Failure modes we design for

| Failure | Behaviour |
|---|---|
| Broker down | Commands fail fast with `unreachable`; app shows a banner; no silent queue growth |
| Postgres down | API returns 503 with `Retry-After`; worker buffers to disk-bounded channel, then drops telemetry |
| Redis down | Presence degrades to "unknown", commands still dispatch; rate limiting fails closed to a lower static limit |
| Device offline | Shadow serves last value; commands queue at most 60 s |
| Bad OTA image | Devices self-rollback on failed boot; campaign halts at >2 % failure |
| Cert expiry | 30/14/7-day alerts to vendor; fleet list flags affected devices |
| Vendor deleted | Soft delete, 30-day grace, then purge job |

## Decisions (ADR-lite)

| # | Decision | Why |
|---|---|---|
| 1 | MQTT 5 over TLS with per-device certs | no shared secrets in firmware, revocation is real |
| 2 | Capability library between product and app | app never ships vendor code; vendor never ships UI |
| 3 | Postgres RLS for tenancy | one forgotten `WHERE` is a data breach otherwise |
| 4 | Two binaries | ingest blast radius |
| 5 | Shadow (desired/reported) instead of fire-and-forget | offline control and honest UI state |
| 6 | Timescale hypertable from day one | retrofitting history is a migration nobody wants |
| 7 | Feature slices in `domain`, traits at the edges | rules engine testable without IO |
| 8 | Expo, not native | one team, two platforms, and OTA JS fixes |
| 9 | EMQX, not our own broker | solved problem |
| 10 | object_store crate, S3-compatible | local MinIO in dev, R2/S3 in prod, zero code change |

## Frontend shape

Both clients are feature-sliced — see [06-FRONTEND.md](06-FRONTEND.md). The rule
that matters here: a screen never calls `fetch`; it calls a slice's query hook,
and the API client is generated from the OpenAPI document the `api` crate emits.
