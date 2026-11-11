# Vision — what Haven is and what it refuses to be

## One sentence

Haven is a licensable IoT platform: a hardware vendor writes firmware on our SDK,
sells a box, and the customer's Haven app finds the box by itself, pairs it, and
controls it — without the vendor writing an app, a cloud, or an account system.

## Who pays

| Customer | What they buy | What they get |
|---|---|---|
| **Vendor** (OEM/ODM) | Per-device licence + cloud plan | SDK, cert chain, product catalog, firmware OTA, fleet analytics, white-label app |
| **Brand** (white-label) | App + cloud, their name | Rebadged Haven app pointed at their vendor's products |
| **Enterprise / property** | Private cloud | Same stack, single-tenant deploy, SSO |

The unit of tenancy is the **vendor**. A vendor owns products; products own
devices; devices live in homes. A customer account can hold devices from many
vendors — that is the Xiaomi Home illusion and it is the reason the app is ours,
not theirs.

## The four actors

**Vendor engineer.** Logs into the portal, defines a product (category + what it
can do), downloads the SDK, flashes firmware, uploads a signed image, ships.
Never touches a customer.

**Customer.** Installs the app, taps `+`, sees a radar of nearby unpaired
devices, taps one, enters Wi-Fi once, names a room. Ninety seconds, no manual,
no hub.

**Installer** (optional). Bulk-provisions a building; scans many devices in a
row, assigns rooms from a floor plan.

**Operator** (us). Watches fleet health, certificate expiry, OTA rollback,
abuse, per-vendor quotas.

## The five flows that define the product

1. **Onboarding a vendor** — create tenant, issue product-signing keys, set
   quotas, generate white-label app config.
2. **Defining a product** — category + capabilities + datapoint mapping. This
   is the whole "will the app render my device correctly" question, and it is
   answered by data, not code.
3. **Provisioning a device** — BLE radar in the app → secure handoff of
   credentials over BLE → device claims itself into a home → device appears.
4. **Controlling a device** — capability command → validated → desired shadow →
   MQTT to device → ack → reported shadow → every open app updates live.
5. **Automating** — trigger (time, state, presence, geofence, webhook) → action
   (device command, scene, notification, HTTP) with conditions, delays, loops.

Everything else — dashboards, energy history, sharing, voice — is built on
these five.

## Non-goals (v1)

- **Not a broker.** We run VerneMQ (Apache-2.0); we do not write MQTT.
- **Not a hub-first product.** Cloud is the control plane; LAN is an
  optimisation, not a second truth.
- **Not a Zigbee/Z-Wave radio stack.** Vendors bring their own radio; our SDK
  sits above it. Matter is an interop path, not the core protocol.
- **Not a general app builder.** Product UI comes from the capability library.
  Vendors pick capabilities; they do not ship React.
- **Not self-hosted-first.** Single-tenant deploy exists, but the design target
  is multi-tenant SaaS.

## Why vendors choose it over Tuya

1. **The thing model is open and versioned.** A product's schema is a file in
   their repo, diffable, and the portal refuses to publish a breaking change
   without a migration.
2. **Certificates are theirs.** Device identity is a cert issued under the
   vendor's intermediate CA, not an opaque token we can revoke unilaterally.
3. **No per-feature ransom.** Local control, OTA, and history are in the base
   plan.
4. **The app is a real app.** Feature-sliced, accessible, offline-tolerant —
   not a WebView.

## Success metrics (the ones we instrument first)

| Metric | Target | Why |
|---|---|---|
| Pair success rate, first attempt | ≥ 95 % | The radar is the product |
| Time from radar tap to device controllable | < 90 s p50 | Xiaomi does ~60 s |
| Command round-trip, cloud path | p95 < 400 ms | Feels instant |
| Command round-trip, LAN path | p95 < 80 ms | Feels local |
| Ingest cost per device-month | < $0.02 | The business model |
| OTA rollout rollback time | < 15 min | A bad image is a fleet outage |
| Crash-free app sessions | ≥ 99.5 % | Store ratings |

## Naming

- Platform: **Haven**
- Cloud API host: `api.haven.dev` (per-tenant: `<vendor>.api.haven.dev`)
- App: white-labelled, internal codename `haven-app`
- SDK: `haven-device` (C component `haven_sdk`, Rust crate `haven-device`)
- Topic root: `haven/`
- Product keys: `hv_<vendor_slug>_<sku>`
