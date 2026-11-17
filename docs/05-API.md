# API surface (v1)

Base `https://api.haven.dev/v1`. JSON only. RFC 3339 timestamps, UUIDv7 ids.

That first sentence is enforced, not aspirational: `make time-serde` (`scripts/time-serde.py`) fails the
gate when a `Serialize` struct exposes an `OffsetDateTime` without
`#[serde(with = "time::serde::rfc3339")]`. It was added after `devices.last_seen_at` shipped as
`[2026,321,3,26,34,…]` — a JavaScript client turns that into `Invalid Date`, and the app rendered "just
now" for a device that had been silent for four hours.
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

### Paging the dashboard

`GET /homes/{id}/dashboard` is unpaged by default and answers with every device — that is the
contract every installed build was written against. A client that may face a large home asks for a
page instead:

```
GET /homes/{id}/dashboard?device_limit=200
→ { …, devices: [...200], devices_total: 431, devices_next_cursor: "0:4c616d70…:01a1…" }

GET /homes/{id}/dashboard?device_limit=200&device_cursor=0:4c616d70…:01a1…
→ { …, devices: [...], devices_next_cursor: null }
```

The cursor is **keyset**, not an offset: it names the last row of the previous page in the home
screen's ordering `(room sort, name, id)`, so a device claimed between two requests cannot make the
next page skip one or repeat one. It is opaque by design — the server may change the ordering without
the client shipping a change — and a cursor this server did not issue is refused with `422` and
`details.field = device_cursor`.

`devices_total` is the home's count, not the page's: a phone showing "200 of 431" needs the 431. Only
the page's points are read, which is half of what paging is for — the payload was 322 kB for a
hundred devices.

Every answer also carries `presence: { total, online, silent }`, counted over the **home's** rows rather
than the page's. The header used to summarise the page it had fetched, so a 1,200-device home read
"200 offline" — a fact about our pagination printed as a fact about the house. `silent` applies the
client's own rule (`max(15 min, 3 × report_interval_secs)`) in SQL, so the header and the tiles cannot
disagree about who has gone quiet.

### Appearance (no session needed)

| Method | Path | Notes |
|---|---|---|
| GET | `/theme` | the platform's brand: `product_name`, `logo_text`, `accent`, `accent_secondary`, `mode`, and the computed `accent_on` |

Public on purpose: the sign-in screen has to be branded before anybody has a token. Cached for
60 seconds (`cache-control: public, max-age=60`) because a brand changes when an operator decides to,
not between two launches. A client that starts offline keeps the last answer it saw; the app caches
it locally and uses it before the first paint.

`accent_on` is **black or white, chosen by the server** from the accent's relative luminance (WCAG's
0.179 cut-off). It is derived, not stored and not accepted on write: two clients working it out for
themselves is two places for a white label to become unreadable.

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

A device also carries `last_seen_at` (RFC 3339) and, when its product declared one,
`report_interval_secs`. Together they are how a client tells "asleep on schedule" from "gone": the rule
is `max(15 min, 3 × report_interval_secs)`, and the platform's own presence sweep uses the same rule, so
a device is never marked offline before the app would call it quiet.

Each point carries `updated_at` too, and that one was missing for a long time. `last_seen_at` answers
"is the device reachable", which is not the same question as "how old is this number": a sensor can hold a
healthy connection and report temperature on an interval, so a device that is online with an eleven-hour-old
reading had to be drawn as if the value were current. The column existed in `device_points` from the first
migration; the API simply never selected it. It is `null` for a point that has never been written, and a
client that gets `null` should say "no reading yet" rather than substitute the device's clock.

### Alerts

| Method | Path | Notes |
|---|---|---|
| GET | `/notifications?unread=&limit=` | newest first, keyed by the caller's user id |
| GET | `/notifications/unread-count` | `{unread}` — a badge needs a count, not 50 rows |
| POST | `/notifications/{id}/read` | `{changed}`; `false` means it was already read |
| POST | `/notifications/read-all` | `{cleared}` |

`notifications` has no vendor column and no RLS: the only thing keeping one
person's alerts apart is the `user_id` predicate, so it is never optional.

Each notification carries a free-form `data` object, and two keys in it are a contract: when an
automation is triggered by a device, `data.device_id` names that device and `data.device_name` gives
it in words, so the app opens it when the alert is tapped and labels the action "Open Room Sensor"
rather than "Open the device". An alert that says "hallway light left on" and cannot be opened is a
notification the customer reads and then goes hunting for. Rows from before this existed carry `{}`
and simply do not offer the jump, which is why the client treats the key as optional.

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
| GET | `/homes/{id}/dashboard` | one call: home, rooms, devices+points, scenes, unread count. Add `device_limit` (1–500) for a page, and `device_cursor` for the next one; a paged answer also carries `devices_total` and, when there is more, `devices_next_cursor` |
| PATCH | `/homes/{id}` | `{name?, geo_lat?, geo_lon?, timezone?}` — coordinates together, `null` clears; `timezone` is an IANA name checked against `pg_timezone_names`, because a wrong zone shifts every schedule silently |
| POST | `/homes/{id}/presence` | `{present}` — this member is home / away |
| PATCH/DELETE | `/rooms/{room_id}?home_id=` | rename; delete leaves its devices unassigned |
| POST | `/automations/{id}/hook?token=` | **no bearer token**: the URL is the credential |
| GET | `/notifications` · POST `/notifications/read` |
| POST | `/push-tokens` |
| GET | `/metrics` | Prometheus text; loopback, or `x-ops-token` elsewhere |
| GET | `/catalog/capabilities` | versioned capability catalog, cached 1 h |
| GET | `/catalog/products?hashes=&keys=` | radar metadata by advertisement hash or product key (exact, 1–50 terms, never an enumeration). Carries `radio` when the product declares it, so the app can warn "2.4 GHz only" before a customer types a password for a 5 GHz network |
| GET | `/stream` | WebSocket upgrade |

### Your data, and leaving

| Method | Path | Notes |
|---|---|---|
| GET | `/me/export` | everything we hold about the caller, as a download: profile, homes, rooms, devices and their last state, scenes, automations, alerts, and the last 7 days of telemetry (≤5000 rows) |
| DELETE | `/me` | erases the account: memberships, alerts, sessions, push tokens, and homes nobody else lives in; anonymises the person |
| GET | `/ops/privacy/due` | `{due}` — how many tombstones are past their window right now; the console's number, and the same one the sweep acts on |
| POST | `/ops/privacy/purge` | deletes tombstones whose retention window has passed, up to 500 per call; returns the ids purged |

An export contains the caller's data only — another member of the same home appears as a role and
a count, never as a person. An erasure is refused with **409** while the caller still owns a home
somebody else lives in, because deleting it would evict a stranger; transfer ownership first. Each
request leaves a receipt in `data_requests`, which survives the account, so "when was this erased"
stays answerable.

The erasure is not the end of it. `licence_grants.retention_days` is the term the vendor bought, and
`DELETE /me` sets `users.purge_after` from the longest window among the vendors whose devices were in
the person's homes — decided once, at erasure, because a renegotiated grant afterwards must not move
the finish line in either direction. The worker sweeps hourly and deletes tombstones whose window has
passed, writing a `purge` receipt as it goes; the ops door does the same on demand, so a backlog can
be cleared without waiting for a tick. Repeats are safe: a tombstone that is not due is not a
candidate.

## Vendor (portal)

Scoped under the vendor of the authenticated principal; a vendor user cannot
address another vendor's resource even with a guessed id (RLS + explicit check).

| Method | Path | Notes |
|---|---|---|
| GET | `/vendor/devices?product_key=&state=&q=&cursor=&limit=` | fleet page; `state` = online/offline/bound/unbound; `q` = substring of serial or name, trimmed, capped at 64 chars, `%`/`_` escaped (trigram index, migration 0041) |
| GET | `/vendor/analytics` | totals, online, bound, new 7/30 d, by product, by firmware |
| GET | `/vendor/audit?limit=&cursor=` | the vendor's append-only trail, newest first |
| GET/POST | `/vendor/products` | |
| GET/PATCH | `/vendor/products/{id}` | |
| DELETE | `/vendor/products/{id}` | `204`; `409` naming the count when devices are live on it or licence activations reference it — both make the entry permanent |
| POST | `/vendor/products/{id}/validate` | the six rules from the thing model |
| POST | `/vendor/products/{id}/publish` | |
| POST | `/vendor/products/{id}/certification/submit` | `{note?}` → asks us to test it; only a published product |
| GET/POST | `/vendor/products/{id}/firmware` | upload → presigned PUT |
| POST | `/vendor/firmware/{id}/sign` | portal signs with the vendor key |
| GET/POST | `/vendor/ota-campaigns` | |
| POST | `/vendor/ota-campaigns/{id}/pause\|resume\|halt` | |
| GET | `/vendor/devices?product_id=&status=` | fleet list, cursor-paged |
| GET | `/vendor/devices/{id}` | telemetry, logs, cert status |
| GET/POST | `/vendor/api-keys` | `POST` mints a key, shown once |
| DELETE | `/vendor/api-keys/{id}` | |
| GET | `/vendor/audit` | |
| GET/POST | `/vendor/webhooks` | `{url, events[]}` → `{id, secret}` once; events: `licence.activated`, `device.claimed`, `firmware.failed` |
| DELETE | `/vendor/webhooks/{id}` | |
| GET/POST | `/vendor/team` | members and invitations |
| DELETE | `/vendor/team/members/{id}` | owner cannot be removed, and the last owner cannot be demoted |
| POST | `/vendor/team/{id}/sso` | binds an identity-provider subject to a member |
| POST | `/vendor/invites/accept` | public: accepts an invitation token |
| GET/PATCH | `/vendor/sso` | `{required, domain, bound_members, total_members, enforced}` |
| GET/PATCH | `/vendor/security` | `{totp_required, members, enrolled}` — owner only; **409** while any member has not enrolled |
| POST | `/vendor/totp/enrol` | `{secret, otpauth_uri}` — shown once, replaces any previous secret |
| POST | `/vendor/totp/confirm` | `{code}` → proves the member can read a code; until then the secret is not a factor |
| DELETE | `/vendor/totp` | turns the member's factor off; **409** while the vendor requires it |
| GET/POST | `/vendor/campaigns` | OTA rollouts; `{id}/start`, `{id}/halt`, `{id}/rollback` |
| GET | `/vendor/licences?limit=` | `{summary:{granted,used,remaining,activated_this_period,granted_value_cents,used_value_cents,unit_price_cents,currency}, activations:[…]}` |
| GET | `/vendor/invoices?period=YYYY-MM` | that month as a `draft`, or the frozen document once issued |
| GET | `/vendor/invoices/{period}` | the same, addressed by month |
| GET | `/ops/invoices?vendor_id=&period=` | one vendor's month, live or frozen |
| POST | `/ops/invoices` | `{vendor_id, period}` → freezes the month; repeating returns the first document |
| GET | `/ops/invoices.csv?vendor_id=&period=` | line-per-device CSV with a TOTAL row, for a finance import |

`POST /vendor/auth/login` takes `{vendor_id, email, password, totp?}`. When the account has confirmed
a second factor, a missing code is `401` with `details.totp = "required"` and a wrong one is `401`
with `details.totp = "invalid"` — different answers, because "ask me for a code" and "try again" send
the portal somewhere different.

## Operator (internal)

### Appearance

| Method | Path | Notes |
|---|---|---|
| GET | `/ops/theme` | the same row the app reads, so the console's preview cannot drift from it |
| PUT | `/ops/theme` | `{product_name, logo_text, accent, accent_secondary, mode}`; returns the saved theme |
| GET | `/ops/theme/history` | last 20 changes: `{changed_at, changed_by, fields}` |

Validation lives in `domain::theme`, so the console and the API cannot disagree about what a colour
is: `#RRGGBB` (or `#RGB`, expanded), `#` required, an unknown `mode` refused. A refusal is `422` with
the offending field named (`error.details.field`), which is what lets the console put the message
beside the input instead of in a toast.

The actor comes from `x-ops-actor` when the console knows who is signed in, and falls back to `ops`.
An unchanged save writes **no** history row — the diff is computed in the domain, so a log of no-ops
never accumulates. Every real change keeps the previous theme in `app_theme_history`, because "who
changed the brand, and what was it before" is not answerable from a single overwritten row.

`/ops/*` — loopback without a token, `OPS_TOKEN` from anywhere else. The same door backs the
**operator console** in the portal at `/admin` (vendors, health, certification queue, billing).

The surface is deliberately small: what changes money and permission (vendors and their quotas,
licence grants, invoices, certification), what explains the platform to itself (health, one device's
state, the conformance lab), and the erasure sweep. There is no endpoint here for metrics, queue depth
or per-vendor rate limits — those live on `/metrics` and in the platform's own configuration, and a
console that duplicated them would be a second source of truth.

| Method | Path | Notes |
|---|---|---|
| GET | `/ops/licences?vendor_id=&limit=` | one vendor's balance and recent activations |
| POST | `/ops/licences` | `{vendor_id, quantity, unit_price_cents, currency?, expires_in_days?, note?}` → `{id, summary}` |
| POST | `/ops/products/{id}/certification/certify` | `{note?}` → the sign-off that lets a device into the app |
| POST | `/ops/products/{id}/certification/revoke` | `{note?}` → new claims refused, existing devices untouched |
| GET | `/ops/audit` | `?vendor=&before=&limit=` → the vendor audit log widened to every vendor; keyset paged on `(ts, id)` so two actions in one millisecond cannot hide one |
| GET | `/ops/overview` | vendors, suspended, awaiting certification, invoices this month |
| GET | `/ops/health` | devices online/total, heard in 15 min, points and commands per hour, notify/webhook backlogs, the five quietest devices and the five stuck commands |
| GET | `/ops/certification/queue` | submitted products across vendors, oldest first |
| GET | `/ops/vendors?limit=` | every vendor with products, devices and licence usage |
| POST | `/ops/vendors` | `{slug, name, quota_devices?}` — onboard a tenant |
| GET | `/ops/vendors/{id}` | one vendor |
| PATCH | `/ops/vendors/{id}` | `{status?, quota_devices?, name?}` — suspend, resume, set or clear a ceiling |
| GET | `/ops/conformance/checklist` | what `scripts/conformance.py` will check, as data |
| POST | `/ops/conformance/sessions` | `{vendor_id, product_id}` → `{product_key, serial, secret, ticket, home_id, allow_unknown_serial}` |
| GET | `/ops/devices/{id}/state?vendor_id=` | the platform's own view: online, last seen, per-point values, recent commands |
| POST | `/ops/conformance/devices/{id}/commands?vendor_id=` | `{code, value}` → issue a command to a device **in the conformance lab only** |

### Invoices

A month's activations are a **draft** until somebody issues them, and then they are a document. The
draft is computed from `licence_activations` on every read, so it is always current; issuing copies
it into `licence_invoices` and `licence_invoice_lines`, and from then on the preview returns the
frozen copy.

That distinction is the whole design. A preview that moves is honest — a device activated late
belongs to that month. An invoice that moves is a bill changing under the person who approved it, so
it cannot: a device activated after the issue belongs to the next period, and the ledger still counts
it (`summary.used` is unchanged by issuing).

Issuing is idempotent (`ON CONFLICT (vendor_id, period) DO NOTHING`, then return the existing
document), because a nightly retry or two operators clicking at once must not produce two bills. A
month that has not started is refused: an invoice for devices that are not activated yet is an
invoice for an intention.

### Suspending a vendor

`PATCH /ops/vendors/{id}` with `{"status": "suspended"}` implements the clause in `docs/11` exactly,
and the wording matters because a lawyer will read it:

- **new activations stop.** The claim path answers `403 vendor_suspended` — a typed refusal that
  names the vendor, not the serial or the money, because both may be perfectly fine;
- **existing devices keep working.** No device row is touched. Their reports, commands and OTA
  continue, because a suspended vendor's customers are still *our* customers;
- **the portal goes read-only.** Any non-`GET` under `/v1/vendor/*` answers `403` while the status is
  not `active`; reading the fleet still works, so the vendor can answer their own support tickets
  while an invoice dispute is sorted out.

A **quota** (`quota_devices`) is the second ceiling, and it is enforced at the same point: when the
live device count reaches it the claim answers `403 quota_reached`, even with licences left. The two
are different questions — a licence is what the vendor bought, a quota is what the operator agreed
they may run — and a console that showed a number doing nothing would be lying. `null` means no
ceiling.

`deleted` exists in the schema and is deliberately unreachable from the console: deleting a tenant
with devices in people's homes is a decision made with a database client and a second person.

### Conformance

`scripts/conformance.py` drives a real device through the contract the app depends on and reads its
answers back from `/ops/devices/{id}/state` rather than from its own log: claim, connect, report,
receive a command, acknowledge it, reconnect, and a serial outside the manifest.

A session creates a device in a home named **Conformance lab**, owned by an account that cannot be
signed into (`conformance+<vendor>@haven.invalid`). It is real data on purpose — the same claim, the
same licence ledger, the same broker — and invisible to every customer. Two rules keep it honest:

- A conformance run **activates a licence**, exactly like a customer's device. No licences left is a
  `402`; the run does not proceed on credit.
- The command door acts as the lab's owner and **refuses any device outside the lab** (`403`), so an
  operator token cannot make somebody's bulb blink.

What "outside the manifest" means is the product's declaration, not the suite's habit: a
manifest-only product must refuse the serial, a trust-on-first-use product must accept it and charge
for it. The run reads `allow_unknown_serial` from the thing model and checks what the product
promised.

### Product certification

Publishing is the vendor saying the product is done; certification is us saying we tested it.
Only a certified product can be claimed, which is what keeps a vendor's unfinished device out
of our consumer app:

```
uncertified --submit--> submitted --certify--> certified --revoke--> revoked
                 ^            |                                        |
                 +--withdraw--+                          submit ------+
```

Every other move is a `409` naming the states — in particular `uncertified → certified`,
because skipping the test is the whole thing the gate exists to prevent. Same-state is a
no-op, so a retried POST is not an error. Revoking stops **new** activations and leaves
devices already in homes alone; a revoked product has to be submitted again, which is what
makes a revoke mean something rather than a toggle.

### Outbound webhooks

A subscription is a URL, a set of events and a secret. Deliveries carry
`x-haven-event`, `x-haven-timestamp`, `x-haven-signature: sha256=<hex>` and
`x-haven-delivery` (stable per event, so a retry is not a second event).

The signature is HMAC-SHA256 over `<timestamp>.<body>` — the timestamp is inside the signature, so
a captured delivery cannot be replayed after the receiver's window closes. Verify with the secret,
and treat anything older than a few minutes as failed.

Events are written to an outbox **in the same transaction** as the fact that caused them, so a slow
or dead endpoint never slows a pairing. Delivery retries with exponential backoff, and stops after
twelve attempts with the last error kept. `http://` endpoints are refused unless they are localhost,
because the secret travels with every request.

### Device licences

The platform charges a vendor **per device that goes live in the app**. The billable
moment is the claim: the activation row is written by the same transaction that takes the
serial out of the manufacturing manifest, so a device cannot exist unbilled and a retried
claim cannot be billed twice (`UNIQUE (product_id, serial)`).

The balance is `grants − activations`, derived on every read rather than stored, so it
cannot drift from the rows it summarises.

A claim with no licence left fails with **402 `licence_exhausted`** — not 403. The caller
is allowed; the vendor has not paid. Nothing about the device is wrong, and the message
says so, because the person holding the phone can only wait for the vendor to buy more.

Expired grants stop counting the moment they expire. Devices already activated keep
working, forever: a customer's bulb is not collateral for a vendor's invoice.

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
| `notification` | `{id}` — an alert for the signed-in person, on `user:{user_id}` |

The socket subscribes to two kinds of channel: one `home:{id}` per home the caller proved membership
of, and `user:{user_id}` for the person themselves — because an alert belongs to whoever has to read
it, and one customer can be in several homes. A `notification` frame carries no `device_id`: the app
bumps its unread count in place and refetches the list, rather than waiting for a poll.

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

An optional **`csr_pem`** (PKCS#10) makes the platform sign a key the device generated, and the
response then has no `private_key_pem` — the key never leaves the device, which is the whole point
of a CSR. The subject, key usages, `clientAuth` EKU and SAN are still written by the platform from
the device row, so a request asking to be called `CN=ca.example` gets a certificate called after
its device id. EC (P-256/P-384) and RSA CSRs both work; every certificate is verified against the
CA with an independent parser before it is returned, so a signing gap becomes a `422` with a
sentence instead of a device that fails to connect.

One CA, and only one: `HAVEN_CA_DIR` is resolved to an absolute path and logged at startup, and
`make` pins it to the repository root for the API, the worker, `make broker-tls` and the broker.
A relative default follows the process's working directory, and an API started from `backend/`
once signed device certificates with a CA the broker had never heard of — which presents as a TLS
alert on the device and looks like a firmware bug.

The CA is self-signed and generated on first use at `HAVEN_CA_DIR` (default `.haven/ca`),
kept on disk with `0600` because a CA key in a database is one backup away from being
everywhere. Each certificate carries the device id as its common name, the vendor as an
organisational unit, and `clientAuth` as its only extended key usage; it has no hostname SANs,
because a device certificate opens a connection and should never be trusted to serve one.

### Broker authentication

The broker asks the platform who a client is, instead of keeping a second copy of the device table.
`vmq_webhooks` POSTs every `auth_on_register` to `POST /v1/internal/mqtt/register` and every
`auth_on_publish` / `auth_on_subscribe` to `POST /v1/internal/mqtt/authorize`, and does what the
answer says. Both require the broker's token as `?token=` — a query parameter because the plugin
cannot send a custom header, and the endpoints are an oracle otherwise.

- A device on the 8884 listener logs in with `username = its device id` and `password = its secret`,
  the same secret it claimed with. The id is minted by the platform, so there is no name a device
  can choose and no ambiguity between two vendors' serials.
- The platform's own clients — the API that publishes commands, the worker that ingests, the
  simulator — use one service identity, `haven-service`, with `HAVEN_MQTT_SERVICE_TOKEN`. It is the
  only identity allowed outside a device's own topic prefix.
- `allow_anonymous` is therefore **off**, and a connection without credentials is refused with
  `5` (not authorized). Verified: a wrong device secret is refused, anonymous is refused, and the
  platform's own traffic (34 e2e assertions) is unaffected.

The topic ACL is **enforced**: a device may publish and subscribe only inside
`haven/<vendor>/<home>/<device>/…`, wildcards and `$SYS` are refused, and the decision is made from
the device row rather than from anything the device asks for. Verified against 8884 over the LAN:

| Attempt | Result |
|---|---|
| correct secret | CONNACK accepted |
| wrong secret / no credentials | CONNACK 5, not authorized |
| publish to its own `evt` | PUBACK |
| subscribe to `haven/#` | SUBACK 128, refused |
| publish to a sibling device's topic | no PUBACK, refused |

That took one setting: **`plugins.vmq_acl = off`**. `vmq_acl` ships enabled, claims
`auth_on_publish` and `auth_on_subscribe` itself, and its default ACL allows everything — so it
answered first and the webhook was never consulted. Register worked throughout because it is a
hook `vmq_acl` does not implement, which is exactly the asymmetry that made the gap look like a
webhook problem. `scripts/mqtt-probe.py` is the reproduction, and it needs QoS 1: a QoS-0 publish
has no acknowledgement, so "refused" and "accepted" look identical.

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

`haven_theme_cache_hits_total` counts theme reads answered without touching the database. The endpoint
is public and every install asks for it on cold start; the row changes when one operator presses Save,
so a process-local cache with a 30-second TTL answers the rest, and a save primes the cache with what
it wrote — the operator's own next render is never the old brand.

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
