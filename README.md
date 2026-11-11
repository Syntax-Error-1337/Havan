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
deploy/         Compose, EMQX config, TLS, observability
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
make up        # postgres+timescale, redis, emqx, minio
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
