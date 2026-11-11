# API surface (v1)

Base `https://api.haven.dev/v1`. JSON only. RFC 3339 timestamps, UUIDv7 ids.
Errors are `{"error":{"code":"…","message":"…","details":{…}}}` with a stable
`code`; `message` is for humans and may change.

Auth: `Authorization: Bearer <access>` — access 15 min, refresh 30 days with
rotation + reuse detection. Vendor endpoints accept either a portal session or a
vendor API key, sent the same way: `Authorization: Bearer hv_<prefix>_<secret>`.
There is no `X-API-Key` header. The prefix identifies the key for revocation; only
the secret is compared, and only its hash is stored.

Revoking is immediate and permanent: the next request with that secret is `401`,
and a revoked key is indistinguishable from one that never existed, so a key cannot
be probed for. Another vendor's key id answers `404`, not `403` — the id is not a
thing you are allowed to know exists.

Known gap, stated rather than implied: keys carry no scopes. A key acts as the
whole vendor account until revoked, which the portal says in as many words.


Pagination: `?limit=&cursor=`; response `{"data":[…],"next_cursor":null}`.

Every mutating request accepts `Idempotency-Key`.

## Customer (app)

### Identity & homes

| Method | Path | Notes |
|---|---|---|
| POST | `/auth/register` | email or phone; returns tokens |
| POST | `/auth/login` | password; 2FA challenge if enabled |
| POST | `/auth/refresh` | rotation; reuse revokes the family |
| POST | `/auth/logout` | revokes refresh family |
| POST | `/auth/otp/request` · `/auth/otp/verify` | phone/email OTP |
| GET | `/me` | profile, homes, feature flags |
| PATCH | `/me` | name, locale, timezone |
| GET | `/homes` | |
| POST | `/homes` | |
| GET/PATCH/DELETE | `/homes/{id}` | |
| GET/POST | `/homes/{id}/members` | invite by email/phone |
| PATCH/DELETE | `/homes/{id}/members/{uid}` | roles: owner, admin, member, guest |
| GET/POST | `/homes/{id}/rooms` | |
| PATCH/DELETE | `/rooms/{id}` | |

### Devices

| Method | Path | Notes |
|---|---|---|
| GET | `/devices?home_id=` | current values included |
| GET | `/devices/{id}` | points, product, firmware |
| PATCH | `/devices/{id}` | name, room |
| DELETE | `/devices/{id}` | unclaim, keeps history 30 d |
| POST | `/devices/{id}/commands` | `{point, value}` or `{action}` → `202 {command_id}` |
| GET | `/devices/{id}/commands/{cid}` | settled state |
| GET | `/devices/{id}/history?point=&from=&to=&bucket=` | Timescale aggregates |
| POST | `/devices/{id}/identify` | blink / beep |
| GET | `/devices/{id}/diagnostics` | rssi, uptime, fw, last seen |

### Pairing

| Method | Path | Notes |
|---|---|---|
| POST | `/homes/{id}/pairing-tickets` | admin; 5-min TTL |
| POST | `/homes/{id}/pairing-sessions` | installer; 2 h |
| GET | `/pairing-sessions/{id}` | progress |
| POST | `/provision/claim` | **device-facing**, no user auth, secret + ticket |

### Automations

| Method | Path | Notes |
|---|---|---|
| GET/POST | `/homes/{id}/automations` | create: `{name, definition}`; admin only |
| PATCH | `/automations/{id}` | `{enabled}` — re-enabling arms a scheduled rule immediately |
| DELETE | `/automations/{id}` | admin only |
| GET | `/automations/{id}/runs?limit=` | newest first, `limit` clamped to 100; the step log is the audit trail |
| GET/POST | `/homes/{id}/scenes` | named action bundles |
| POST | `/scenes/{id}/activate` | |

**Not built yet:** `POST /automations/{id}/run` (run now). The scheduler can
already run any rule whose `next_run_at` is due, so this is a small endpoint —
it is held back because a manual run would be recorded as a `schedule` run, and
a mislabelled audit trail is worse than no button.

### Manufacturing manifest

| Method | Path | Notes |
|---|---|---|
| GET/POST | `/vendor/products/{id}/serials` | `{serials:[…]}` → `{added, already_listed, summary}` |
| GET | `/vendor/products/{id}/serials` | `{total, claimed, open}` |

### Vendor API keys

| Method | Path | Notes |
|---|---|---|
| GET/POST | `/vendor/api-keys` | create: `{name}` → `201` with the secret **once** |
| DELETE | `/vendor/api-keys/{id}` | revoke; `204`, and the key stops working immediately |

A key is `hv_<8 hex prefix>_<32 hex secret>`. The prefix is stored in the clear
for lookup; the secret is stored as SHA-256 and shown exactly once. Any vendor
route accepts it: `Authorization: Bearer hv_…`, alongside the session token.

SHA-256 rather than Argon2 is deliberate — a key is 128 bits of randomness, so
there is nothing for a slow hash to defend. Passwords remain Argon2. A key has no
user behind it: `user_id` is the nil uuid, and revocation (not expiry) is how it
stops working. Scopes are stored as `{vendor}` but **not yet enforced** — every
key is currently equivalent to a vendor session.

### OTA

| Method | Path | Notes |
|---|---|---|
| GET/POST | `/vendor/products/{id}/firmware` | register an image already uploaded and signed; `sha256` must be 64 hex |
| POST | `/vendor/campaigns` | `{product_id, firmware_id, rollout_pct}` — defaults to **0%**, not 100% |
| GET | `/vendor/campaigns/{id}` | status, `started_at` |
| POST | `/vendor/campaigns/{id}/start` | freezes the wave, queues jobs, returns `{queued}` |

A campaign closes itself (`complete`) once every job is final; a campaign that
matched **no** devices stays `running` on purpose — closing it would hide a
mistake behind a green tick. The worker sweeps jobs that have been silent for 45
minutes: back to `pending` while attempts remain (3 total), then `failed` with a
reason, so a device that never answers cannot keep a rollout "in progress" forever.

The image does not travel through the API: the vendor uploads it and signs it
themselves, and the device verifies the signature before writing flash. The
worker claims queued jobs and publishes an `OtaOffer` on the device's own `ota`
topic; it never moves bytes.

**Rollout is stable, not sampled.** Membership is `hash(campaign, device) % 100 <
pct`, so a device cannot drift in and out of a wave between scans, and raising the
percentage only ever adds devices (`domain::ota`).

### History

| Method | Path | Notes |
|---|---|---|
| GET | `/devices/{id}/history?code=&range=` | downsampled to ≤60 buckets; `range` is `1h\|1d\|7d\|30d` |

One bucket per slice, each carrying the **average** and the sample count. For a
boolean the average is the fraction of the window it was true, which is what a
"when is the light on" chart should draw. A bucket with no samples is drawn as a
gap, never as zero — silence is not a reading.

### Alerts

| Method | Path | Notes |
|---|---|---|
| GET | `/notifications?unread=&limit=` | newest first, keyed by the caller's user id |
| GET | `/notifications/unread-count` | `{unread}` — a badge needs a count, not 50 rows |
| POST | `/notifications/{id}/read` | `{changed}`; `false` means it was already read |
| POST | `/notifications/read-all` | `{cleared}` |

`notifications` has no vendor column and no RLS: the only thing keeping one
person's alerts apart is the `user_id` predicate, so it is never optional.

### Scenes

| Method | Path | Notes |
|---|---|---|
| GET/POST | `/homes/{id}/scenes` | create: `{name, icon?, actions}`; admin only |
| DELETE | `/scenes/{id}` | |
| POST | `/scenes/{id}/activate` | runs every action, returns `{dispatched, failed}` |

A scene has **no trigger and no conditions** — that is the distinction from an
automation. "Good night" is a button; adding a trigger would mean two engines
that overlap. Every action goes through the same command path a manual press
uses, so a scene cannot invent a second way for a device to be told something.
One dead device does not stop the rest: failures come back per action rather than
aborting the burst.

### Misc

| Method | Path |
|---|---|
| GET | `/homes/{id}/dashboard` | one call: rooms, devices, points, automations, alerts |
| GET | `/notifications` · POST `/notifications/read` |
| POST | `/push-tokens` |
| GET | `/catalog/capabilities` | versioned capability catalog, cached 1 h |
| GET | `/catalog/products?product_key=` | radar metadata by product hash |
| GET | `/stream` | WebSocket upgrade |

## Vendor (portal)

Scoped under the vendor of the authenticated principal; a vendor user cannot
address another vendor's resource even with a guessed id (RLS + explicit check).

| Method | Path | Notes |
|---|---|---|
| GET/POST | `/vendor/products` | |
| GET/PATCH | `/vendor/products/{id}` | |
| POST | `/vendor/products/{id}/validate` | the six rules from the thing model |
| POST | `/vendor/products/{id}/publish` | |
| GET/POST | `/vendor/products/{id}/firmware` | upload → presigned PUT |
| POST | `/vendor/firmware/{id}/sign` | portal signs with the vendor key |
| GET/POST | `/vendor/ota-campaigns` | |
| POST | `/vendor/ota-campaigns/{id}/pause\|resume\|halt` | |
| GET | `/vendor/devices?product_id=&status=` | fleet list, cursor-paged |
| GET | `/vendor/devices/{id}` | telemetry, logs, cert status |
| GET | `/vendor/analytics/overview` | devices, activity, errors, pair success |
| GET/POST | `/vendor/api-keys` | |
| GET | `/vendor/webhooks` · POST/DELETE | event subscriptions |
| GET | `/vendor/sdk` | SDK versions + download links |
| GET/PUT | `/vendor/branding` | white-label app config |
| GET | `/vendor/audit` | |

Key issuance for mTLS lives here too: `POST /vendor/ca/intermediate` returns a
CSR-signing endpoint and the vendor's CA cert; the vendor keeps the private key.

## Operator (internal)

`/ops/*` — guarded by `OPS_TOKEN`, loopback-bound unless explicitly exposed.
Tenant CRUD, quotas, cert expiry report, campaign halt-all, job queue depth,
ingest lag, per-vendor token buckets.

## WebSocket protocol

Connect `wss://api.haven.dev/v1/stream`, then send the first frame. It carries
the token **and** the homes to watch, so the server can subscribe before it
answers — Redis pub/sub has no replay, and a `sub` frame after `ready` would
race the first event:

```json
{"t":"auth","token":"<access>","homes":["h_01H…"]}
```

Server replies `{"t":"ready","homes":["h_01H…"]}` once the subscriptions are
live, or `{"t":"error","code":"invalid_token"|"no_homes"|"auth_required"}`.
Membership is checked per home; a home the caller cannot prove is dropped
silently rather than announced.

Events (server → client):

| `t` | Data |
|---|---|
| `point.changed` | `{device_id, code, value, ts}` |
| `device.online` / `device.offline` | `{device_id, ts}` |
| `device.added` / `device.removed` | `{device}` |
| `command.settled` | `{command_id, status, reported, code}` |
| `automation.fired` | `{automation_id, run_id}` |
| `ota.progress` | `{device_id, state, pct}` |
| `home.updated` | `{home}` |

Heartbeat: the server answers protocol-level pings; a client that misses two is
dropped. The client reconnects with jittered backoff and re-sends `auth`.
Reconnect with jittered backoff 1→30 s and re-subscribe; the client refetches
`/homes/{id}/dashboard` on reconnect rather than trusting missed deltas.

## Conventions that are non-negotiable

1. **`202` for commands**, never `200`. The command is accepted, not applied.
2. **`409` for state conflicts** (already claimed, transfer pending), `422` for
   semantic validation, `400` for malformed JSON.
3. **No secrets in URLs.** Tokens go in headers or the WS first frame.
4. **Every list is cursor-paginated**, even when it is small today.
5. **OpenAPI is generated** (`utoipa`) and committed at `docs/openapi.json`;
   the app and portal clients are generated from it, never hand-written twice.
