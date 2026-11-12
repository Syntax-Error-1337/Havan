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
migrations/     SQL migrations (applied by `haven-migrate`)
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

## Running it

```bash
make up        # postgres, valkey, vernemq, seaweedfs
make migrate   # schema
make api       # :8080
make worker    # MQTT ingest + schedulers
make sim       # virtual devices
make app       # Expo
make portal    # Vite
```

## What we borrow, and from where

Gladys (Apache-2.0) is the reference for **product shape**, not code: the
device→feature→state model, the dashboard/scene UX, the integration-catalog
mental model. Everything here is a rewrite in Rust/TypeScript with a different
tenancy and provisioning story, because Gladys is a single-home self-hosted
assistant and this is a multi-tenant commercial cloud.

Apache-2.0 material that is copied verbatim keeps its notice (see `NOTICE`).

## Signing in on a fresh checkout

**Vendor portal** (`make portal`) — seeded, deterministic, and printed by the seed:

| | |
|---|---|
| email | `admin@acme.test` |
| password | `acme-demo-password` |
| vendor id | `11111111-1111-7111-8111-111111111111` (slug `acme`) |

`make seed` creates that vendor, that user and three published products. The vendor
id is a constant rather than a fresh UUID per run, because an identity that changes
every seed cannot be documented — and the portal's login form pre-fills it.

**Customer app** — there is deliberately no seeded account. Customers register
themselves (`POST /v1/auth/register {email, password}`, or "Create one" on the login
screen), then create a home. A shared demo password in a customer app is the one
credential that must not exist: it is the account a device is bound to.

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

