# Roadmap

Each phase ends with something demonstrable and a commit. "Done" means the
acceptance check ran, not that the code exists.

## Phase 0 — Skeleton (day 1–2)

Workspace, protocol crate, kernel, migrations, compose stack, health endpoint,
CI. **Done:** `make up && make migrate && cargo test --workspace` green, `/healthz`
returns `{db, mqtt, redis}` statuses.

## Phase 1 — Identity, homes, devices (day 3–6)

Register/login/refresh, homes, members, rooms. Device CRUD against a product
catalog row. Dashboard endpoint. **Done:** a user can create a home, a room, and
see an empty dashboard; RLS proven by a test that vendor A cannot read vendor B.

## Phase 2 — Catalog, claim, shadow (day 7–11)

Products (seeded with 3 reference SKUs), capability catalog, pairing tickets,
`/provision/claim`, device + points creation, MQTT ingest, shadow write, WS
`point.changed`. **Done:** the simulator claims a device with a ticket and its
state appears in the app's dashboard within 1 s.

## Phase 3 — Commands (day 12–14)

Command endpoint, validation, idempotency, desired shadow, publish, ack
handling, settle, WS `command.settled`. **Done:** toggle a light from the app;
the sim acks; the UI settles; a rejected value shows the device's message.

## Phase 4 — App MVP (day 15–22)

Expo app: onboarding, radar (BLE), Wi-Fi form, dashboard, device detail with
capability renderers, scenes (list + run), settings. **Done:** on a real phone,
radar → paired → toggled, in under 90 s, with the sim in place of hardware.

## Phase 5 — Vendor portal (day 23–28)

Product editor with thing-model validation, firmware upload + sign, fleet list,
basic analytics. **Done:** a new product can be defined, validated, published,
and used by the simulator end to end.

## Phase 6 — Automations (day 29–35)

Trigger/condition/action DSL, evaluator, scheduler, run log. Migration of the
Gladys scene vocabulary (time, device state, presence, webhook) into the DSL.
**Done:** "if motion after sunset and nobody home → turn on hallway light and
notify" runs with a readable per-step log.

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
| Timescale ingest cost | unit economics | history policy per capability, compression after 24 h |
| Vendor thing-model mistakes | broken UI for real customers | six validation rules + reference SKUs + cert gate |
| Scope creep into a general app builder | never ships | capability catalog is closed; new capability = platform release |
| Two clients diverge | double maintenance | shared OpenAPI client, shared design tokens, same FSD lint |
