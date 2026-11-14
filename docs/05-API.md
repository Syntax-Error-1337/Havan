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

### Firmware (customer)

| Method | Path | Notes |
|---|---|---|
| GET | `/devices/{id}/updates` | `{current, target, state, error, at, in_progress}` |

Deliberately tiny: no campaign id, no rollout percentage, no other devices. The
job table is vendor-scoped and this route runs on a system context *after* the
ownership check, so a customer sees the outcome of their own device's update and
nothing else.

### Rollouts

| Method | Path | Notes |
|---|---|---|
| POST | `/vendor/campaigns/{id}/start` | draft → running, queues the wave |
| POST | `/vendor/campaigns/{id}/halt` | draft or running → halted; returns `{skipped}` |

Halting drops the queued jobs (`skipped`) and leaves in-flight ones alone: a
device half-way through a flash must finish or fail on its own schedule, and
killing it mid-write is how firmware updates brick hardware. A halted campaign
reports `halted` with `Stopped · N skipped`, never `Done` — the progress maths
counts skipped jobs as finished, and the label must not claim the fleet updated.

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
| GET | `/homes/{id}/dashboard` | one call: home, rooms, devices+points, scenes, unread count |
| PATCH | `/homes/{id}` | `{name?, geo_lat?, geo_lon?}` — coordinates together, `null` clears |
| POST | `/homes/{id}/presence` | `{present}` — this member is home / away |
| PATCH/DELETE | `/rooms/{room_id}?home_id=` | rename; delete leaves its devices unassigned |
| POST | `/automations/{id}/hook?token=` | **no bearer token**: the URL is the credential |
| GET | `/notifications` · POST `/notifications/read` |
| POST | `/push-tokens` |
| GET | `/metrics` | Prometheus text; loopback, or `x-ops-token` elsewhere |
| GET | `/catalog/capabilities` | versioned capability catalog, cached 1 h |
| GET | `/catalog/products?product_key=` | radar metadata by product hash |
| GET | `/stream` | WebSocket upgrade |

## Vendor (portal)

Scoped under the vendor of the authenticated principal; a vendor user cannot
address another vendor's resource even with a guessed id (RLS + explicit check).

| Method | Path | Notes |
|---|---|---|
| GET | `/vendor/devices?product_key=&state=&cursor=&limit=` | fleet page; `state` = online/offline/bound/unbound |
| GET | `/vendor/analytics` | totals, online, bound, new 7/30 d, by product, by firmware |
| GET | `/vendor/audit?limit=&cursor=` | the vendor's append-only trail, newest first |
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

## Presence, location and webhooks

**Presence is declared, not inferred.** `POST /homes/{id}/presence {"present": false}`
marks *you* away; the automation condition "nobody home" reads the aggregate of the
home's members. A geofence that guesses wrong turns a light on in an empty flat and
there is nobody to argue with — so the app has a switch and the automation reads it.

**Solar rules need a place.** `PATCH /homes/{id}` sets `geo_lat`/`geo_lon` (both
together, `null` clears). Sunrise and sunset are computed locally with the NOAA
approximation (`domain::solar`) — accurate to a minute, no timezone database, and polar
day/night returns *no answer* rather than midnight. Creating a solar automation without
coordinates is a `422` with a sentence, not a rule that fires at 00:00 forever.

**Webhooks are armed, not executed.** `POST /automations/{id}/hook?token=…` sets
`next_run_at = now()` and answers `202`; the worker runs it on its next tick, so there
is exactly one executor. The token is generated server-side at creation. A wrong token
and a missing automation both answer `404`: an anonymous caller must not be able to
enumerate rules by watching which one answers differently.

**Vendor reads carry no customer identity.** `/vendor/devices` and `/vendor/analytics`
are scoped to the vendor and return serial numbers, state, firmware and last-seen —
not the owner, not the home name. Pagination is keyset on `(created_at, id)`; an
unknown `state` filter is `422` rather than an empty page that looks like "no devices".

## Alerts, and pushing them to a phone

`GET /notifications`, `POST /notifications/{id}/read`, `POST /notifications/read-all` —
the in-app list, unchanged.

`POST /notifications/tokens` with `{platform: "ios"|"android", token, app_version}` registers
a device token (idempotent — an app registers on every launch), and
`DELETE /notifications/tokens/{token}` removes it on sign-out, so the next person holding the
phone does not receive the previous owner's alerts. A platform outside those two and a token
shorter than 16 characters are refused here rather than stored and retried forever.

The app registers on sign-in and on relaunch (`getDevicePushTokenAsync`, after asking for
permission once), and removes the token on sign-out — a phone that keeps its registration after
someone signs out receives the previous owner's alerts. A refused permission is an answer, not
an error: registration is skipped and the app stays usable.

A worker sweep delivers undelivered notifications every thirty seconds:

- a notification with **no tokens** is marked delivered, not left pending — there is nobody to
  tell and a queue that keeps such rows grows without bound;
- **unregistered** from the provider prunes the token and settles the message;
- **any other error** counts an attempt, and after three the row is settled as undeliverable
  with the reason kept.

APNs uses token (`.p8`) authentication over HTTP/2: an ES256 JWT, `apns-topic` from
`HAVEN_APNS_TOPIC`, `apns-push-type: alert`. Enable it with `HAVEN_APNS_KEY_FILE`,
`HAVEN_APNS_KEY_ID`, `HAVEN_APNS_TEAM_ID` and optionally `HAVEN_APNS_ENDPOINT` (defaults to
the sandbox). **Without those credentials the worker uses a log transport** and says so at
startup — a deployment that believes it is pushing while it is only logging is a support
ticket, so the transport is named in the first line of every worker start.

## Device certificates

`POST /provision/certificate` with `{pk, sn, secret}` — the same credentials the device
claimed with, because the claim ticket is single-use and short-lived. Answers `201` with
`{device_id, certificate_pem, private_key_pem, ca_pem, serial, not_after}`.

The CA is self-signed and generated on first use at `HAVEN_CA_DIR` (default `.haven/ca`),
kept on disk with `0600` because a CA key in a database is one backup away from being
everywhere. Each certificate carries the device id as its common name, the vendor as an
organisational unit, and `clientAuth` as its only extended key usage; it has no hostname SANs,
because a device certificate opens a connection and should never be trusted to serve one.

A device connects to the broker on **8883** with that certificate; the listener requires one
signed by this CA, so a device without it cannot publish at all. `make broker-tls` writes the
broker's own certificate, and `make up` runs it, so a fresh checkout needs no extra step.

The private key is generated server-side and returned once. A device that can produce a CSR
should keep its own key; the API stores neither, only the serial and `cert_not_after`.

A worker sweep warns the home's members at **30, 14, 7 and 0 days** left
(`notifications.kind = 'security'`, deduped per device and bucket). An expired certificate is
a device that stops connecting and looks like broken hardware, which is why the warning has
to arrive while someone can still act on it.

## Sending a command

`POST /devices/{id}/commands` answers `202` with `{ command_id, status }` — recorded and
dispatched, not applied. Only the device's ack settles it.

With the broker unavailable it answers **`503`**, naming the command: `the device broker is
unavailable; command <id> was not sent`. The command is recorded as
`unreachable / broker_unavailable` with `attempts = 1`, and the worker **redelivers it** if
the broker returns within two minutes (`dispatched`, `attempts = 2`). Past that window it
stays `unreachable` and the client retries — a command replayed hours later would act on an
intention nobody still holds. `haven_commands_undelivered_total` counts the refusals.

## Rolling back a bad image

`POST /vendor/campaigns/{id}/rollback` answers `201` with
`{ campaign_id, from_version, to_version, queued }`.

It is deliberately **not an undo**. Devices that already flashed the bad image have to be
told to flash again, so a rollback creates a second campaign — already running, because a
rollback that waits for someone to press start is a rollback that does not happen at 3 a.m.
— and queues **only the devices currently reporting that campaign's version**. A fleet that
never took the image is left alone.

The target is chosen in this order:

1. the version **most of the fleet is actually running**, among firmware older than the bad
   one — putting three devices back beside the hundred that never left is what a person
   means by "roll back";
2. failing that, the newest image uploaded before the bad one.

Never a *newer* image: rolling forward to a hotfix is a new campaign, and calling it a
rollback would put the wrong word in the audit trail. If neither rule finds anything the
request is refused with `422` rather than queuing nothing and reporting success.

A rollback is recorded as `campaign.rollback_started`, with both versions, the count and
the campaign it reacted to.

## The audit trail

Administrative actions append a row to `audit_log`: publishing a product, minting or
revoking an API key, starting or halting a rollout. Each row carries the vendor, the
actor (`user` / `api_key` / `system`), the action in the past tense
(`api_key.revoked`), the target, the request id, the source address and a small JSON
detail.

It is **append-only by construction**: the module has no update or delete path and the
migration grants neither — an audit row that can be edited is not evidence. Reads are the
vendor's own, keyset-paginated on `(ts, id)`, and row-level security means one vendor
cannot see or write into another's trail.

Not a request log, deliberately: a table that records every read is a table nobody can
afford to keep, and `/metrics` already answers "how many, how slow".

## Rate limits

Limits count **failures**, not requests. A request counter cannot tell a person who has
forgotten which password this is from a script working through a list, and only one of
them is a problem — so a legitimate sign-in never meets the limit.

| Endpoint | Key | Failures per minute |
|---|---|---|
| `POST /auth/login` | source IP + email | 10 |
| `POST /vendor/auth/login` | source IP + email | 10 |
| `POST /provision/claim` | source IP + serial | 10 |
| `POST /auth/register` | source IP | 20 |

Over the limit answers `429` with `{"error":{"code":"rate_limited","message":"…"}}`, and
the refusal is counted in `haven_rate_limited_total`. The counter lives in Valkey (one
`INCR` plus an `EXPIRE` on the first hit), so every replica shares one window. If the
cache is unreachable the check **fails open**: a cache outage must not lock people out of
their own homes, and the request is still recorded in the audit path.

## Metrics

`GET /metrics` renders Prometheus text (content type `text/plain; version=0.0.4`) with:

| Metric | Type | Labels |
|---|---|---|
| `haven_http_requests_total` | counter | `route` (matched path), `status` |
| `haven_http_request_duration_seconds` | histogram | `route` |
| `haven_commands_dispatched_total` | counter | — |
| `haven_ws_connections` | gauge | — |
| `haven_rate_limited_total` | counter | — |
| `haven_uptime_seconds` | gauge | — |

The `route` label is the **matched** path (`/v1/devices/{id}`), never the raw URL:
labelling by raw path turns one metric into one per device id, which is how a metrics
endpoint becomes the outage it was meant to detect. The endpoint answers on loopback
without a token, and everywhere else only with `x-ops-token` matching `OPS_TOKEN`
(compared in constant time). With no `OPS_TOKEN` set it is unreachable from off-machine
rather than open.

## Conventions that are non-negotiable

1. **`202` for commands**, never `200`. The command is accepted, not applied.
2. **`409` for state conflicts** (already claimed, transfer pending), `422` for
   semantic validation, `400` for malformed JSON.
3. **No secrets in URLs.** Tokens go in headers or the WS first frame.
4. **Every list is cursor-paginated**, even when it is small today.
5. **One client, generated not hand-written.** There is no OpenAPI spec yet:
   `utoipa` is not a dependency and nothing is generated, so the app and portal
   each call this surface directly and `docs/05-API.md` is the contract between
   them. A spec and generated clients belong with the SDK work, not before it.
