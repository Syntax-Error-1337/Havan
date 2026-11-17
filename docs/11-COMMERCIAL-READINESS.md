# 11 — Commercial readiness: one app, per-device licences

> Revised after the business model was stated plainly: **one customer app** (the Haven app) in which
> a customer connects devices from any vendor, and the platform operator charges each vendor **per
> device licence**. That is the Smart Life model, not the Xiaomi model — the app is ours, the
> hardware is theirs, and revenue is per activated device.
>
> Every "missing" claim below was checked by search at `caa4fd8`; each P0/P1 item is meant to be
> expanded into a task-level plan before it is built.
>
> **Status:** every P0 and P1 item here has landed — licences and billing views (`0020`–`0021`),
> certification (`0022`), the vendor-response guard, backups and the restore drill, export/delete
> (`0023`), vendor team invitations (`0025`), outbound webhooks (`0026`), the support bundle, firmware
> signing enforcement, and SSO configuration (`0027`, ADR-005).
>
> Still open, and named so nobody has to rediscover them: the **portal screens** for the balance,
> certification and team; **SSO enforcement**, which waits for a real identity provider (ADR-005);
> a **live APNs call**, which needs an Apple account and a device; and the hardware certification in
> `docs/10-RELEASE-CHECKLIST.md`.

## The two relationships, and who is not the customer

- **Vendor → operator.** A vendor pays per device that goes live in our app. Not a subscription, not
  a seat, not a per-app licence. This is the revenue line.
- **Operator → consumer.** The consumer pays us nothing directly; they buy hardware. The consumer is
  the *user*, and their experience is the product quality bar.
- **The consumer must never be collateral.** If a vendor stops paying, devices already shipped keep
  working for the people who bought them, and that vendor simply cannot activate new ones. Bricking a
  customer's bulb over an unpaid invoice is the one failure mode that ends a platform's reputation.

A useful consequence: **the app is not white-labelled per vendor.** It is the platform's app — one
brand, set once by the operator — presenting every vendor's devices coherently. What a vendor buys is
placement in a working app and a certified device, not their own icon on a stranger's home screen.

Who the *operator* is, though, is a different question, and that one **is** white-label: a partner who
licences the platform ships it under their own name and colours. That is why the brand is data —
`app_theme` plus `GET /v1/theme` — and not a constant in the app's source. The console's Appearance
page writes it; every phone reads it before sign-in. Per-vendor branding inside a shared home is
deliberately still out: two vendors' colours on one customer's screen is a question about whose brand
wins, and it deserves a decision rather than a default.

## What already exists, and it is more than the first draft of this plan assumed

`product_serials` (`backend/migrations/*serial_manifest.sql`) is already a licence ledger in embryo:

```
product_serials (vendor_id, product_id, serial, manufactured_at, claimed_at, device_id)
+ product_serials_open_idx  WHERE claimed_at IS NULL   -- "how many are left"
```

A serial row is one manufactured unit; `claimed_at IS NULL` means "on the shelf, not yet billable";
`claimed_at` is the moment it is consumed. Row-level security already scopes it per vendor. The
migration's own comment says the manifest exists to make "this serial exists" a fact rather than a
trust-on-first-use claim — which is exactly the property a billable unit needs.

So the work is not to invent a licence system. It is to put **money and a gate** around this one.

## Where each item landed

The sections below are written as they were planned — "add a balance", "add a gate" — which reads like
a to-do list. This table is the state, so the plan is not mistaken for the backlog:

| # | Item | State | Where |
|---|---|---|---|
| 1 | Licence grants and the activation ledger | **done** | `0020`–`0021`, `domain/licence.rs`, `GET/POST /ops/licences`, `GET /vendor/licences` |
| 2 | Billing views and invoices | **done** | `0030`, `/vendor/invoices`, `POST /ops/invoices`, `/ops/invoices.csv` (`draft` until issued, frozen after) |
| 3 | Certification before a device is claimable | **done** | `0022`, `POST /vendor/products/{id}/certification/submit`, `/ops/products/{id}/certification/{certify,revoke}` |
| 4 | A vendor never sees our customers | **done, and the guard enumerates the routers** | `api/tests/vendor_shapes.rs` — 25 vendor routes, every one either fetched or explained |
| 5 | Backups that are known to restore | **done** | `scripts/backup.sh`, `scripts/restore-drill.sh`; transcript in `docs/09` |
| 6 | Data lifecycle | **done** | `0023`–`0024`, `0029`, `/me/export`, `DELETE /me`, retention from `licence_grants.retention_days`, hourly purge + `POST /ops/privacy/purge` |
| 7 | Vendor teams, SSO, TOTP | **done except SSO enforcement** (needs an IdP to validate an assertion against) | `0025`, `0027`, `0031`, `/vendor/team`, `/vendor/sso`, `/vendor/security`, `/vendor/totp/*` |
| 8 | Outbound events | **done** | `0026`, `worker/webhooks.rs`, `webhook_outbox`, HMAC-signed with backoff |
| 9 | Support surface | **done** | `GET /vendor/support-bundle` (allowlist, never a redacted row) |
| 10 | Firmware signing and rollback | **done** | `api/tests/firmware_signing.rs`, campaign gate + `docs/10` §2 |

Beyond the ten: a **device conformance suite** (`scripts/conformance.py`, 7/7 live) so certification
is something we run rather than something we assert, and an **operator console** (`/admin` in the
portal) with the vendor book, certification queue, billing and platform health.

### Flows found missing by walking both consoles as a user

Every route below existed before this pass and no screen called it, so each was a dead end rather than
a missing feature. They are listed because "the endpoint exists" was what made them look done.

| Flow | What was wrong | Where it landed |
|---|---|---|
| Bind an identity for SSO | the guard refuses enforcement while anybody is unbound, and nothing could bind — `enforced` could never be true | Security page member table; `POST /vendor/team/{id}/sso`, empty subject unbinds (409 while required) |
| Delete a product | no DELETE existed at all | `DELETE /vendor/products/{id}` — 409 naming the count when devices are live or activations reference it, since those cascade from `products` |
| Attach the support bundle | the artefact a vendor must send us was curl-only | `/support` in the portal, through `download()` in the API client |
| See the DSAR purge | the operator could delete data and not tell whether it kept up | `GET /ops/privacy/due` + `/admin/privacy` |
| Read the operator's own audit | suspend and certify left no readable record across vendors | `GET /ops/audit` + `/admin/audit` |
| Export invoices as CSV | the link sent no ops token, so it worked on loopback and would 401 anywhere real | button through `adminDownload`; the route names the file |
| Say what radio a product needs | nothing declared 2.4 GHz-only, so pairing failed in a customer's living room | `ThingModel.radio`, validated against closed lists, carried by the radar lookup |


## P0 — the revenue engine

**1. Licence grants: a vendor must own the right to activate before they can.** Today a vendor uploads
a manifest of any size and claims freely: nothing is paid, nothing stops them. Add:

- `licence_grants (vendor_id, quantity, unit_price_cents, currency, granted_at, expires_at, note)` —
  a balance, incremented by an operator action and decremented on activation.
- `licence_activations (vendor_id, product_id, serial, device_id, activated_at, period)` — written at
  the same moment `product_serials.claimed_at` is set, in the same transaction. This is the invoice
  source of truth: the manifest keeps the shelf, the activation keeps the ledger.
- Balance check on the claim path: zero balance → typed refusal `402 licence_exhausted`, naming the
  balance, the way `quota_devices` was supposed to work.

Charge at **activation**, not at manifest upload. Activation is where value is delivered, it is
already an atomic state transition — `infra/src/repos/pairing.rs:45` takes the serial with
`UPDATE product_serials SET claimed_at = now() WHERE … AND claimed_at IS NULL` inside a transaction,
so the activation row can be written by that same transaction with no new failure mode — and a vendor
who manufactures a run and sells half of it owes for half, which is the argument that closes a first
deal.

Acceptance: a vendor with zero balance claims a device and gets `402 licence_exhausted`; the operator
grants 10; the same claim succeeds; `licence_activations` gains exactly one row for that serial.

**2. Billing views on both sides.** Vendors need "what will I owe"; the operator needs "what is owed
to me".

- `GET /v1/vendor/licences` — granted, consumed, remaining, activations this period, unit price.
- Operator view: per-vendor balance, activations per period, invoice-ready export.
- An invoice is a nightly roll-up of `licence_activations` for a period. Starting as an export is
  fine; the ledger is the hard part, and it must be idempotent per serial, not per request.

Acceptance: a retried claim of the same serial produces one activation row and one invoice line.

**3. Certification before a device is claimable in Haven's app.** Today a product goes
`draft → published`, and a published product's serials can be claimed by anyone holding a ticket. In
a one-app world that means a vendor's unfinished device surfaces in *our* consumer app and the
consumer blames us. This is the gate that protects the brand, and the difference between a platform
and a marketplace of bugs.

Add a `certification` state on the product (`uncertified → submitted → certified → revoked`) and a
checklist the operator signs off: protocol conformance (claim, shadow, ack, typed refusal),
capability manifest matches what the device actually reports, OTA path exercised once, certificate
and secret handling, and a physical test on a real unit. Claims for an uncertified product are
refused; devices already claimed keep working.

Acceptance: a published-but-uncertified product refuses claim with a typed error; certifying it lets
the same claim through without a redeploy.

**4. One-app governance: a vendor sees their devices, never our customers.** Homes, users and the
consumer's identity belong to the operator. The vendor's legitimate view is their own devices' health
and telemetry — not who lives in the house.

This is currently clean, and the point of the item is that it stays clean. `domain/src/fleet.rs:21`
`FleetDevice` carries `id, product_key, product_name, name, serial, state, bound, online, fw_version,
last_seen_at, created_at` — no home name, no owner identifier, no address, and `presence_and_geo` is
per home and absent from any vendor response. Row-level security already scopes by vendor; the risk
is a future *join* that pulls a customer column into a vendor endpoint, which no code review reliably
catches.

So the deliverable is a guard, not a fix: a test that serialises every `/v1/vendor/*` response shape
and fails if a home name, owner identifier or coordinate appears. It belongs in the suite before the
first vendor's data is in the database, because after that the test is retro-fitted around real
responses.

## P0 — the parts that are not about money

**5. Backups that are known to restore.** `scripts/` has `chaos.sh`, `load.sh`, `e2e.sh` and no
`backup.sh` or `restore-drill.sh`. A vendor's security questionnaire asks this by name, and a restore
path nobody has run is a hypothesis.

Build `scripts/backup.sh` (dump to `.partial`, verify with `pg_restore --list`, rename into the set,
stream retention) and `scripts/restore-drill.sh` (throwaway database, per-table counts, hypertables
survived, dropped even on failure). Paste the transcript into `docs/09-RESILIENCE.md`.

**6. Data lifecycle.** `dsar` returns **0 files**; retention is per-point, not per-tenant. A vendor
reselling us to an EU customer inherits the obligation.

`GET /v1/me/export` (homes, devices, scenes, automations, a bounded telemetry window) and
`DELETE /v1/me` (soft delete plus scheduled purge), both audited, with the retention window taken
from the vendor's licence row.

## P1 — what the first three vendors ask for after signing

**7. Vendor teams.** `vendor_users` has four roles and TOTP, but `invite` exists only for *home
members*, `saml` appears nowhere, and nothing enforces TOTP. Add invite/accept with expiring tokens,
a permission matrix applied at the route layer, and TOTP enforcement as a vendor policy.

**8. Outbound events.** Every `webhook` hit today is broker auth. A vendor integrating their ERP needs
"device activated", "licence low", "firmware failed" pushed to them, signed with HMAC and retried with
the backoff shape the push worker already uses. No subscriber must never block a write.

**9. Support surface.** `support_bundle` → 0 files. Build bundles by allowlist (named fields into a
fresh document, never a redacted row) plus a fault table the diagnostics screen reads.

**10. Firmware signing and rollback.** Still open in `docs/10-RELEASE-CHECKLIST.md`. In a one-app
world an unsigned OTA is our app shipping a bad update into a stranger's house.

## P2 — after two vendors are live

- **Capacity in licence terms**: `docs/08-PERFORMANCE.md` measured ~600–700 msg/s/worker; turn that
  into "N activated devices per worker" so a vendor's growth is plannable.
- **Integrations that sell hardware**: Matter bridge and voice (Alexa/Google) — already after-v1 in
  the roadmap, and the two that get devices onto retail shelves.
- **On-prem single-tenant** with a signed licence file, for operators who cannot use a shared cloud.
  Same binaries and a config profile; the work is packaging, not product.
- **SOC 2 readiness**: artifacts fall out of the audit trail that exists; the work is writing them
  down.

## Order

Wave 1 (items 1–4) is the business: a vendor buys licences, activates devices, cannot exceed what
they bought; a device reaches the app only after we certify it; a vendor never sees our customers.
Wave 2 (5–6) makes us insurable. Wave 3 (7–10) makes us supportable. Wave 4 is packaging and
partnerships.

## Two decisions that are yours, not mine

1. **Prepaid packs or metered invoice?** Prepaid ("buy 10,000 activations") is what vendors expect and
   collects cash first; metered is friendlier to a startup but puts you in the collections business.
   The ledger above supports either — it is a pricing decision, not a schema one.
2. **What a suspended vendor loses.** Recommendation: new activations stop, existing devices keep
   working, the portal goes read-only. Confirm before it is coded, because it is the clause a vendor's
   lawyer reads first.
