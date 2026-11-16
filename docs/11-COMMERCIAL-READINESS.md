# 11 — Commercial readiness: what a paying vendor needs next

> Written after reading the code at `caa4fd8`, the 19 migrations, the 16 route modules and the
> project graph. Every "missing" claim below was checked by search; the command is named where it
> matters. This is a business plan, not an implementation plan — each P0/P1 item is meant to be
> expanded into a task-level plan before it is built.

## Who pays, and for what

Three buyers, and they want different things from the same platform:

1. **A hardware startup** with a product and no cloud. They buy the whole stack — cloud, app, portal,
   SDK — and ship *their* brand. They care about time-to-market and that the app is not obviously
   someone else's.
2. **An established OEM** with an existing cloud they are replacing. They care about SSO, data
   residency, SLAs, an audit trail, and an exit plan. They will not sign without a backup/restore
   drill and a security questionnaire.
3. **An enterprise with no internet** (utilities, industrial, campuses). They buy a single-tenant
   deployment and a licence key, not a subscription.

The current build serves (1) well and (2) partially. Nothing in it serves (3).

## What exists today

Verified, not remembered: 19 migrations (`tenants`, `identity`, `devices`, `automation`,
`device_product`, `ticket_scope`, `point_history`, `vendor_sessions`, `session_subject`, `ota_pct`,
`ota_job_error`, `scenes`, `serial_manifest`, `telemetry_point_idx`, `presence_and_geo`, `audit_log`,
`command_delivery`, `device_certificates`, `push_delivery`). The `vendors` table already carries
`quota_devices`, `branding jsonb`, `status` and a `slug`; `vendor_users` carries four roles plus
TOTP. Cloud API with 16 route modules, vendor portal with 9 screens, customer app, OTA campaigns,
device certificate issuance, push delivery, audit trail, API keys, OpenAPI document.

That is a genuinely strong v1. The gaps below are not features missing from the product — they are
the parts of *being a business* that a product does not give you.

## P0 — blocks the first paying vendor

**1. Metering and billing.** `billing`, `invoice`, `entitlement` and `metering` return **0 files**.
`quota_devices` is an integer that nothing enforces or measures, and `vendor_users.role` has no
permission matrix behind it. So today the platform cannot answer "what does this customer owe" or
"has this customer outgrown their plan".

Build: a `plans` table (device ceiling, message ceiling, retention window), `usage_counters` rolled
up per vendor per period from data already flowing (`device_points`, `commands`), a read-only
`GET /v1/vendor/usage` for the portal, and enforcement at the *edges* — claim refuses past quota,
ingest sheds past the message ceiling — never in the query path. Invoicing can start as a nightly
export; the counter is the hard part and it must be idempotent.

Acceptance: claim the (quota+1)th device and get a typed refusal naming the plan; a vendor's own
portal shows current usage against the ceiling without a support ticket.

**2. White-label delivery that reaches the user's phone.** `branding` exists as jsonb and the app
reads tokens from `app/src/shared/config/tokens.ts`, but `custom_domain` returns **0 files** and the
push path holds **one** APNs identity. A vendor shipping under their own App Store listing needs
their own bundle id, their own APNs key, their own icon set and splash — per vendor, not per
deployment.

Build: per-vendor push credentials (encrypted column or vault entry, keyed by vendor, selected at
send time), a build-time branding pipeline (EAS profiles per vendor from `branding`), and a
`GET /v1/vendor/branding` + portal screen to edit it. Custom domain is the same shape for the portal.

Acceptance: two vendors with different APNs keys both receive a push from the same worker, and
neither sees the other's certificate in a support bundle.

**3. Backups that are known to restore.** `ls scripts/` shows `chaos.sh`, `load.sh`, `e2e.sh` — and
no `backup.sh` or `restore-drill.sh`. A hosted platform whose restore path has never been run is
uninsurable, and the security questionnaire asks for it by name.

Build: `scripts/backup.sh` (dump to `.partial`, verify with `pg_restore --list`, then rename into the
set) and `scripts/restore-drill.sh` (restore into a throwaway database, compare per-table counts,
assert hypertables survived, drop it). Paste the drill output into `docs/09-RESILIENCE.md`.

Acceptance: the drill passes against the current database and the transcript is committed.

**4. Data lifecycle: export and delete.** `dsar` returns **0 files**, and retention is a per-point
policy rather than a per-tenant one. Under GDPR a customer's request has a deadline, and "we will
write a query" is not an answer a vendor can resell.

Build: `GET /v1/me/export` (a JSON bundle of the caller's homes, devices, scenes, automations and a
bounded telemetry window) and `DELETE /v1/me` (soft-delete plus a scheduled purge), both audited,
plus a per-vendor retention window in the plan row from item 1.

Acceptance: an export round-trips through a fresh account's import in a test; a delete removes the
rows a subsequent export would have contained.

## P1 — needed by the first three customers

**5. Vendor team management.** `vendor_users` has roles and TOTP but `invite` only appears in the
*home member* flow; there is no vendor-user invite, no SSO (`saml` → 0 files), no policy that
enforces TOTP. An OEM will ask for all three in the first call.

Build: invite + accept flow with expiring tokens, a permission matrix applied at the route layer
(not per handler), TOTP-enforcement policy on the vendor row, and SAML as a second phase behind the
same session model.

**6. Outbound events for vendor systems.** Every `webhook` hit is broker auth. A vendor integrating
their ERP needs "device claimed", "firmware failed", "command refused" delivered *to them*, with
retries and a signature.

Build: a `vendor_webhooks` table (url, secret, event types) and a delivery worker reusing the push
delivery's retry/backoff shape, signing with HMAC. Absence of a subscriber must not block a write.

**7. Support surface.** `support_bundle` → 0 files. Support is where a licensed platform earns its
renewal, and "send us your logs" does not scale.

Build: a bundle endpoint built by allowlist — named fields into a fresh document, never a serialised
row with redactions — plus a fault table (subject, severity, fix action) that diagnostics can read.

**8. Firmware signing and rollback.** Already an open gap in `docs/10-RELEASE-CHECKLIST.md` (Phase 7).
A vendor shipping OTA without a signed image and a rollback path is one bad release from a recall.

## P2 — scale, and the third buyer

- **Single-tenant deployment + offline licence key**, for buyer (3). Same binaries, a different
  config profile and a signed licence file; the work is packaging, not product.
- **Capacity translated into plan terms.** `docs/08-PERFORMANCE.md` measured ~600–700 msg/s/worker;
  "N devices per plan on M workers" is the sentence a buyer needs, and it should live in the plan row.
- **Integrations**: Matter bridge and voice (Alexa/Google) are already listed as after-v1 in the
  roadmap, and they are the two that get a platform onto a retail shelf.
- **SOC 2 readiness**: the artifacts (access review, change management, incident log) fall out of the
  audit trail that already exists; the work is writing them down, not building them.

## Order, and why

Wave 1 (P0 items 1–4) is one coherent story: *a vendor can sign up, be metered, be billed, ship under
their own name, and survive an audit*. Nothing in wave 1 depends on wave 2.

Wave 2 (items 5–8) turns a vendor into a team and a platform into something supportable.

Wave 3 is packaging and partnerships, and it should not start before two vendors are live, because
both of them will change what "capacity" and "integration" mean.

## Deliberately not yet

Camera streaming, energy dashboard, LAN fast path, offline local hub. All are nice revenue stories
and none of them blocks a first contract — and each one is a subsystem, not a feature.

## What would make me wrong

If the first buyer is a startup that only needs a demo to raise a round, wave 1 collapses to items 1
and 2 (metering can be a spreadsheet) and item 3 (they will still ask). Check the buyer before
building the bill.
