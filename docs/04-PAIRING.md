# Pairing — from radar to controllable device

The radar is the product. Everything in this document exists so that a customer
who has never read a manual gets from `+` to a working bulb in under 90 seconds,
and a vendor cannot accidentally make that impossible.

## The sequence

```
app                     cloud                      device
 │                        │                          │  advertising (BLE)
 │ 1 POST /pairing/tickets│                          │
 │───────────────────────►│                          │
 │◄── {ticket, expires}   │                          │
 │                        │                          │
 │ 2 BLE scan ────────────────────────────────────────►│  (service data)
 │ 3 connect + INFO ─────────────────────────────────►│
 │ 4 {t:provision, ssid, pass, ticket, endpoint} ────►│
 │                        │ 5 POST /provision/claim  │
 │                        │◄─────────────────────────│  (serial, secret, ticket, CSR)
 │                        │ 6 verify secret+ticket   │
 │                        │ 7 create device, sign CSR│
 │                        │─────────────────────────►│  {device_id, cert, mqtt}
 │                        │ 8 retained shadow        │
 │◄── WS device.added ────│◄─────────────────────────│
 │ 9 render controls from product capabilities        │
```

Steps 1–2 happen while the radar is already spinning; the ticket is minted when
the user taps a device, not when they open the screen, so the list is not stale.

## Ticket

`POST /v1/homes/{home_id}/pairing-tickets` → `201`

```json
{ "ticket": "hv1.eyJ….<sig>",
  "expires_at": "2026-11-11T23:41:00Z",
  "transport": ["ble","ap","qr"] }
```

The ticket is a signed, opaque blob (Ed25519, 5-minute TTL) carrying:

```
{jti, home_id, issuer_user_id, nonce, iat, exp, scope:"pair", max_devices:1}
```

Properties that matter:

- **It is not a bearer token for the home.** It can only create one device in
  one home, within five minutes, and only from an unclaimed serial.
- **One device per ticket.** The nonce is consumed at claim time in the same
  transaction that creates the device; a second claim with the same ticket
  returns `409` (after the 10-minute idempotency window for the *same* serial).
- **It travels over BLE or HTTPS only.** It is never put in a QR code for the
  BLE path; QR labels carry a factory claim code instead, so a photographed
  label cannot mint devices into arbitrary homes.
- **Minting requires `role >= admin`** on the home except for the owner, who can
  grant `member` pairing rights per home (a guest should not add hardware).

## Device states

```
unprovisioned ──provision──► joining ──claim ok──► claimed ──reset──► unprovisioned
      ▲                          │                   │
      └────────claim fail────────┘                   └───unclaim/transfer──► released
```

`released` (ownership transfer) keeps the device row, history and automations
intact but clears `home_id`, rotates the secret, and re-arms advertising. The
old owner keeps read-only history for 30 days; the new owner starts from the
moment of transfer. Transfer requires both sides to confirm: the current owner
initiates, the new owner accepts within 24 h, and the device must be online at
accept time — otherwise a stolen device is a transfer.

## Server-side invariants

| Invariant | Enforced where |
|---|---|
| A device belongs to exactly one home | `devices.home_id` unique per `(vendor_id, serial)` |
| A serial can be claimed once | `UNIQUE (product_id, serial)` on claim insert |
| Points are created from the product, never from the device | `domain::pairing::on_claim` |
| A device cannot claim into another vendor's home | ticket carries vendor; claim checks `product.vendor_id == ticket.vendor_id` |
| Replay of a claim does not duplicate | idempotency on `(serial, ticket_jti)` for 10 min |
| An unlisted serial cannot claim | `products.serial_manifest` must contain it, or the product has `allow_unknown_serial` |

## Pairing UX states (the app must be honest at every step)

| State | Copy | Radar |
|---|---|---|
| `scanning` | "Looking for devices…" | animated pulse |
| `found` | "Acme Bulb · 1 of 3" | card with icon + RSSI bars |
| `connecting` | "Connecting…" | card locks, others dim |
| `wifi` | "Enter your Wi-Fi" | form, password reveal, 2.4/5 GHz hint if the product is 2.4-only |
| `joining` | "Joining your network…" | step list: Wi-Fi ✓ · Cloud · Done |
| `claiming` | "Almost there…" | progress |
| `done` | "Acme Bulb is ready" | room picker, "Add another" |
| `failed` | specific: wrong password / no internet / device busy / not supported | retry, or switch to AP mode |

Failure copy is specific on purpose. "Pairing failed" makes support tickets;
"Your Wi-Fi password was rejected" does not.

## Radar implementation notes

- Scan via `react-native-ble-plx` with a **service-UUID filter**, not a name
  filter — the OS then does the filtering and the battery cost drops.
- iOS has no `startScan` duration; enforce a 12 s budget client-side.
- Android 12+ needs `BLUETOOTH_SCAN` with `neverForLocation`; do not request
  location unless the product needs it, because the prompt scares users.
- Deduplicate by `(product hash, serial tail)` across advertisements, keep the
  strongest RSSI, cap the list at 10 and sort by signal.
- Known devices in the same home that are already claimed MUST NOT appear; a
  claimed device stops advertising, so this is the device's job, not a filter.
- If BLE permission is denied, fall back to QR/AP and say why.

## Security review of the flow (the four attacks that matter)

| Attack | Mitigation |
|---|---|
| Neighbour re-pairs your bulb | device refuses `provision` while claimed unless the physical reset gesture ran |
| Attacker mints devices into your home | ticket requires admin role, 5-min TTL, single use, never in QR |
| Stolen factory secret | per-device secret, hashed at rest, single-use claim, rate limited by serial and IP, manufacturing manifest checked |
| Evil-twin AP during provisioning | app never sends the Wi-Fi password anywhere but the BLE/AP link to a device it discovered in that session; the endpoint and CA are pinned in the app build |

Not solved in v1 and documented as such: a physical attacker who can hold the
reset button owns the device. That is true of every consumer platform.

## Bulk / installer mode

`POST /v1/homes/{home_id}/pairing-sessions` creates a session that mints tickets
on demand (one per device, no user tap) for 2 hours, bound to an installer
account and audited per device. The app's installer screen scans continuously
and assigns rooms from a floor plan. Rate limit: 60 devices per 10 minutes per
session; above that, the session is paused and emailed to the home owner.

## The manufacturing manifest

`allow_unknown_serial: true` means "trust the first claim for any serial". That is
a prototype default and it is what makes a device row something a caller can
invent, so a product that ships should say **false** and upload the serials it
built.

- `POST /v1/vendor/products/{id}/serials {"serials": ["SN-1", "SN-2"]}` — one
  batch, idempotent, returns `{added, already_listed, summary}`. Serials are
  trimmed, blanks dropped and duplicates folded, because a manifest pasted out of
  a spreadsheet has all three.
- `GET /v1/vendor/products/{id}/serials` → `{total, claimed, open}`. `open` is
  what is still on the shelf.
- Claiming takes a serial in a single conditional `UPDATE … WHERE claimed_at IS
  NULL`, so two devices presenting the same serial cannot both win.
- A serial that is not in the manifest is `404 unknown_serial`. A serial already
  taken is `409 serial_already_claimed`. A serial whose device row exists with a
  different secret is `401 bad_secret` — two devices claiming one serial with
  different secrets is indistinguishable from a stolen serial, and the second one
  loses either way.

Uploading is a card on the product page: paste the list (newlines, commas or a
spreadsheet column all work), and it says how many of the serials are still
unclaimed — which is the number a vendor actually wants.

Still not built: a file picker. Pasting 20,000 lines into a textarea works and is
the same code path a drop zone would feed, so a file input is a convenience on top
rather than a missing capability.

