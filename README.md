# Haven

White-label IoT platform for hardware vendors — the stack behind products like
Tuya and Xiaomi Home, built to be licensed.

```
 Vendor builds device ──► SDK ──► ships to customer
                                    │
 Customer opens Haven app ──► radar finds device ──► pair ──► control
```

## The four surfaces

| Surface | What it is | Stack |
|---|---|---|
| **Haven Cloud** | Multi-tenant control plane + data plane | Rust (axum, sqlx, tokio) |
| **Haven SDK** | Firmware library a vendor builds into a device | C (ESP-IDF/FreeRTOS), Rust reference, simulator |
| **Haven App** | The customer's phone app | Expo / React Native (FSD) |
| **Haven Portal** | Where vendors define products, ship firmware, watch fleets | React / Vite (FSD) |

## Repository layout

```
backend/        Rust workspace — API, worker, protocol, domain slices, infra
app/            Customer app (Expo, feature-sliced)
portal/         Vendor portal (React, feature-sliced)
sdk/            Device-side SDK: C component, Rust crate, shared protocol docs
deploy/         Compose, VerneMQ config, TLS, observability
docs/           Spec, architecture, contracts, roadmap
backend/migrations/  SQL migrations (applied by `haven-migrate`)
```

## Docs

Read in order:

1. [00-VISION.md](docs/00-VISION.md) — what we sell, to whom, and where the money is
2. [01-ARCHITECTURE.md](docs/01-ARCHITECTURE.md) — services, data flow, tenancy, scale
3. [02-THING-MODEL.md](docs/02-THING-MODEL.md) — products, capabilities, datapoints
4. [03-SDK-CONTRACT.md](docs/03-SDK-CONTRACT.md) — the protocol vendors build against
5. [04-PAIRING.md](docs/04-PAIRING.md) — radar, claim tickets, provisioning
6. [05-API.md](docs/05-API.md) — REST + WebSocket surface
7. [06-FRONTEND.md](docs/06-FRONTEND.md) — feature-sliced rules for app and portal
8. [07-ROADMAP.md](docs/07-ROADMAP.md) — phases and what "done" means
9. [08-PERFORMANCE.md](docs/08-PERFORMANCE.md) — measured timings, and the method behind them
10. [09-RESILIENCE.md](docs/09-RESILIENCE.md) — what each outage does, and the four bugs it found
11. [10-RELEASE-CHECKLIST.md](docs/10-RELEASE-CHECKLIST.md) — the gates, and the three items that need hardware

## Running it

```bash
make up        # postgres, valkey, vernemq, seaweedfs
make migrate   # schema
make api       # :8080
make worker    # MQTT ingest + schedulers
make sim       # virtual devices
make app       # Expo
make portal    # Vite

make seed      # demo vendor, portal login, three products
make db-reset  # drop, re-migrate, re-seed
make check     # fmt + clippy + the whole suite, on haven_test
make e2e       # the customer journey over HTTP, against the running stack
make bench     # read-path timings on a fresh 100-device home
make licences  # every dependency is permissively licensed (AGENTS.md, ADR-002)
make load      # ingest throughput and loss (RATE= SECONDS=)
make chaos     # stop Redis/MQTT/Postgres in turn and assert the degraded behaviour
               # (WHAT=redis|mqtt|db to run one). Leaves the stack running.
```

`make db-reset` terminates the connections it finds (`DROP DATABASE … WITH (FORCE)`):
the API and worker hold pools, and a reset that only works when everything is
stopped is a reset nobody runs. Both reconnect on their next query.

## What we borrow, and from where

Gladys (Apache-2.0) is the reference for **product shape**, not code: the
device→feature→state model, the dashboard/scene UX, the integration-catalog
mental model. Everything here is a rewrite in Rust/TypeScript with a different
tenancy and provisioning story, because Gladys is a single-home self-hosted
assistant and this is a multi-tenant commercial cloud.

Apache-2.0 material that is copied verbatim keeps its notice (see `NOTICE`).

## Credentials

Everything below is a **development** credential and is deliberately obvious. It is
also all of them: if a value is not in these tables it lives in `.env` (copied from
`.env.example`), which is never committed.

### Signing in

| Where | Email | Password | Extra |
|---|---|---|---|
| Vendor portal (`make portal`) | `admin@acme.test` | `acme-demo-password` | vendor id `11111111-1111-7111-8111-111111111111` (slug `acme`) |
| Customer app (`make app`) | *none — register* | *none* | `POST /v1/auth/register {email, password}` |
| API (any client) | — | — | `Authorization: Bearer <access>` from `/v1/auth/login` |

`make seed` creates the vendor, that portal user and three published products; it
prints the login it made. The vendor id is a constant rather than a fresh UUID per
run, because an identity that changes every seed cannot be documented — and the
portal's login form pre-fills it.

There is no default **customer** account on purpose: the customer account is the one
a device is bound to, and a password everyone knows is not a password.

### Services

| Service | Address | User | Password | Notes |
|---|---|---|---|---|
| Postgres 16 | `localhost:5433` | `haven` | `haven` | owner: migrations and DDL |
| Postgres 16 | `localhost:5433` | `haven_app` | `haven_app` | the app's role — RLS applies to it |
| Postgres 16 | `localhost:5433` | `haven_system` | *(none)* | `NOLOGIN BYPASSRLS`, reachable only by `SET ROLE` |
| Valkey (Redis) | `localhost:6380` | — | — | no auth; loopback only |
| VerneMQ (MQTT) | `localhost:1883`, TLS `:8883` | — | — | anonymous in dev; inspect with `docker exec haven-mqtt vmq-admin cluster show` |
| SeaweedFS (S3) | `http://localhost:9000` | — | — | no access control; console `:9001` |
| VerneMQ HTTP listener | `localhost:8888` | — | — | reserved for auth webhooks; serves nothing yet |
| Object storage keys | — | `S3_ACCESS_KEY` | `S3_SECRET_KEY` | in `.env` |

Database names: `haven` (dev), `haven_test` (suites). TLS is terminated in front of
the broker in production; the dev listener is plaintext on purpose.

### URLs

| What | URL |
|---|---|
| API | `http://127.0.0.1:8080` (`/healthz`; `/metrics` is Phase 8 and not built yet) |
| App, browser | `http://localhost:8082` (`make app-web`) |
| App, Metro / Expo Go | `exp://<lan-ip>:8081` (`make app`) |
| Vendor portal | `http://127.0.0.1:5173` (`make portal`) |

Every published dev port is bound to `127.0.0.1`. Valkey has no password, VerneMQ
allows anonymous clients and SeaweedFS has no access control — publishing them on
`0.0.0.0` would hand them to every machine on the same Wi-Fi. A device that needs
the broker from the LAN gets an explicit mapping, added when that device exists.

Secrets are not listed here: `HAVEN_JWT_SECRET`, `HAVEN_VAULT_KEY`,
`HAVEN_TICKET_SECRET` and the S3 keys live in `.env` (32+ bytes / 64 hex chars).
A process that starts without them fails at boot rather than serving with a blank
key.

## What CI proves

Four jobs on every push: `backend` (fmt, clippy, migrations, the full suite),
`contract` (the SDK types against their tests), `app`, `portal`.

Two details in `backend` are the difference between a green tick and a useful one:

- **The suite runs as `haven_app`, not as the owner.** `deploy/postgres/init.sql`
  creates the two roles in CI exactly as it does in production, and `DATABASE_URL`
  points at the ordinary login. Run as the owner, row-level security is bypassed and
  every tenancy policy could be wrong while the suite stayed green — which is
  precisely what happened in this repo's first phase.
- **A skipped suite fails the build.** Integration tests return early with
  "skipping" when a database, Redis or the broker is absent. That is right on a
  laptop and wrong on a runner, so CI greps for it and exits non-zero. Without that
  check, a missing service silently leaves the websocket and OTA paths untested.

`make ci-backend` reproduces it locally against a scratch database.

