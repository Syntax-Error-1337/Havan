# Roadmap

Each phase ends with something demonstrable and a commit. "Done" means the
acceptance check ran, not that the code exists.

## Phase 0 — Skeleton (day 1–2)

Workspace, protocol crate, kernel, migrations, compose stack, health endpoint,
CI. **Done:** `make up && make migrate && cargo test --workspace` green, `/healthz`
returns `{db, mqtt, redis}` statuses.

## Phase 1 — Identity, homes, devices (day 3–6) — **done**

Register/login/refresh, homes, members, rooms (create, rename, delete), device CRUD
against a product catalog row, `GET /homes/{id}/dashboard`.

**Done:** a user can create a home, a room, and see an empty dashboard; RLS proven —
`api/tests/dashboard.rs` shows a stranger receiving `404` from the dashboard and from a
room rename, and `ci-backend` runs the whole suite as `haven_app` so the policies bite.

## Phase 2 — Catalog, claim, shadow (day 7–11)

Products (seeded with 3 reference SKUs), capability catalog, pairing tickets,
`/provision/claim`, device + points creation, MQTT ingest, shadow write, WS
`point.changed`. **Done:** the simulator claims a device with a ticket and its
state appears in the app's dashboard within 1 s.

## Phase 3 — Commands (day 12–14)

Command endpoint, validation, idempotency, desired shadow, publish, ack
handling, settle, WS `command.settled`. **Done:** toggle a light from the app;
the sim acks; the UI settles; a rejected value shows the device's message.

## Phase 4 — App MVP (day 15–22) — **done, except the phone pass**

Expo app: onboarding (register → create home), radar (BLE), Wi-Fi form, dashboard
(one request, grouped rows), device detail with capability renderers (bool → switch,
numeric → stepper bounded by the point), scenes, alerts, settings.

**Done:** every screen exists and is exercised by jest (63 tests) and by the browser
build. **Not done:** the 90-second phone pass — this machine has CommandLineTools, not
Xcode, so no simulator or device run. The acceptance step is the first thing to run on
a Mac with Xcode, and `npm run start:mcp` gives the agent screenshot/tap tools there.

## Phase 5 — Vendor portal (day 23–28) — **done**

Product editor with thing-model validation, firmware upload, OTA campaigns (with
halt), API keys, serial manifest, **fleet list** and **analytics**.

**Done:** a new product can be defined, validated, published and used by the
simulator end to end; a vendor can then see every device they shipped — bound or in
stock, online or not, which firmware — and the counts behind it. Verified against the
dev fleet: 104 devices, 50 online, one product, firmware split including devices that
have never reported a version.

## Phase 6 — Automations (day 29–35) — **done**

Trigger/condition/action DSL, evaluator, scheduler, run log. Triggers: `schedule`,
`device_state`, **`solar`** (sunrise/sunset ± offset), **`presence`**, **`webhook`**,
`manual`. Conditions: `device_state`, `time_of_day`, **`present`**, **`solar`**.
Actions: `set_device`, `notify`.

Sunrise and sunset come from a NOAA solar calculation over the home's coordinates
(`domain::solar`), not a timezone database: the sun is an instant and an instant does
not need a zone. Polar day and night return "no answer" rather than midnight.

**Done:** "if motion after sunset and nobody home → turn on hallway light and
notify" builds, arms and runs with a readable per-step log. Verified live: a solar rule
is refused without a location, arms for 15:48Z (London sunset −30 min) once the home
has coordinates; a webhook fires `202`, executes as `trigger_kind = webhook` with
`[trigger, action]` steps and produces the notification.

## Phase 7 — SDK and certification (day 36–45)

C component on ESP32-S3 reference board, Rust crate, `haven-cert`. **Done:** the
reference firmware passes checklist items 1–8 and the portal shows the badge.

## Phase 8 — Hardening (day 46–55)

Load test to 20k events/s, OTA campaign with rollback, cert expiry alerts,
rate limits, audit log, observability (traces, RED metrics). **Done:** chaos run
(broker killed, DB killed, Redis killed) with the documented behaviour observed.

## Explicitly after v1

LAN fast path, Matter bridge, voice (Alexa/Google), energy dashboard, camera
streaming (WebRTC), offline local hub, on-prem single-tenant deploy.

## Risks

| Risk | Impact | Mitigation |
|---|---|---|
| BLE pairing flakiness on Android OEMs | the whole funnel | AP fallback from day one, Playwright-style device matrix, 12 s budget |
| Telemetry ingest cost | unit economics | history policy per capability, partition drop, rollups |
| Vendor thing-model mistakes | broken UI for real customers | six validation rules + reference SKUs + cert gate |
| Scope creep into a general app builder | never ships | capability catalog is closed; new capability = platform release |
| Two clients diverge | double maintenance | shared OpenAPI client, shared design tokens, same FSD lint |
