# 13 — Screen audit, November 2026

Every screen, walked at 390 × 844 in both appearances, with the DOM read for what is actually pressable. What follows is
what was **found**, not what was hoped for: a control that does nothing, a fact with no feedback, a section that should
exist and does not.

Method: `apple-design` (accessibility → platform conventions → craft → interaction → writing), the live app at
`localhost:8082` under CDP device emulation, and the source where the screenshot could not settle a question. Where a
finding was fixed in this pass it says **done**; everything else is a task with a size.

---

## The two that were real bugs

**Rooms opened an editor with an empty field.** `RoomRow` passed `value=""` to `EditableRow`, whose editor seeds its
draft from that value — so tapping "Kitchen" opened a **blank** field over a room called Kitchen, and pressing Save on a
blank field did nothing, silently. Renaming a room therefore meant retyping its name from memory, with no way to tell a
saved rename from a missed tap. — **done**: a sheet that arrives pre-filled, with Save disabled until the name differs.

**No mutation in the app said it happened.** Rename a home, save a location, make somebody an admin, remove a device,
clear the alerts: the row changed and nothing else did. On a slow connection that is indistinguishable from a tap that
missed. — **done**: `shared/ui/Toast.tsx`, an accent-dotted pill above the tab bar with `accessibilityLiveRegion`, wired
to home name, time zone, location, room rename and delete, device rename and room move, invitation, role change, removal
and leaving.

---

## Screen by screen

### Dashboard (`/`)
| Finding | Priority |
| --- | --- |
| Empty state, hero, tiles, dial, search, wide layout — all rebuilt in waves 1–5 | — |
| **An all-offline home dims every field**, so the live look is unreachable — *done as a tool*: `scripts/demo-fresh.sh` moves `last_seen_at`, `online` and every point's `updated_at` forward. It is a demo tool on purpose: a production system must never claim a silent device is live | done |
| **No collapsed/expanded control for a long room** — *done*: a room pages at 8 tiles and ends with "Show N more in <room>" | done |

### Device detail (`/device/[id]`)
| Finding | Priority |
| --- | --- |
| Rename, room move, ranges, chart, remove — present, and now with confirmation | done |
| **No identity block**: serial, product, vendor, firmware version and last-seen time are all in the payload and none are on screen. A support call cannot start with "what is this device" | **P1** |
| No "identify" action for a device you cannot see (blink, beep), and no undo after remove — both need something the platform does not have: an `identify` data point the firmware would answer, and a restore verb on the server. Recorded rather than faked with a toggle that blinks a light for two seconds | blocked |
| Remove is immediate after confirm, with no undo | P2 |

### Add a device (`/add-device`)
| Finding | Priority |
| --- | --- |
| Handoff explains itself, prints the link, offers copy — after this pass' fixes | done |
| The scan screen's nearby-device rows are plain rows; the radar animation is the only art | P2 |
| **No "the device is not appearing" path** — *done*: a collapsed list of the seven checks this hardware actually enforces (2.4 GHz only, WPA2-PSK AES, a previously-claimed device stops advertising, guest-network isolation, two metres, permissions), on both the scanning screen and the browser handoff | done |

### Alerts (`/alerts`)
| Finding | Priority |
| --- | --- |
| Mark all read; opening one marks it read | — |
| **No filter** (all / unread) — *done*: an "Only unread" switch over the loaded feed, with "Nothing unread — you are caught up." for the empty case. Per-alert dismiss is **done**: `DELETE /notifications/{id}`, a swipe with an accessible action, and the home named on each row when the account has more than one | done |
| No empty state distinct from "no alerts at all" vs "all read" | P2 |
| Grouping is by day, which is right; nothing distinguishes homes for a person in two | P2 |

### Automations (`/automations`)
| Finding | Priority |
| --- | --- |
| Empty state, rule rows, run/history/delete, toggle — present | — |
| A rule row shows its schedule as text but has no "last ran at" — *done*: the list query folds in each rule's latest run with a lateral join (one index seek per rule, not a request per row), and the row prints "Last ran 7m ago". Silent for a rule that has never fired | done |
| **No condition editor beyond presence/solar** — *done in part*: "Only between two times" now sends `Condition::TimeOfDay`, with a window that crosses midnight explained in place. Numeric comparisons on a device point are still a curl call | P2 |

### Scenes (`/scenes`)
| Finding | Priority |
| --- | --- |
| Create, run, delete; the form carries the accent on an empty screen | done |
| **No rename** — *done*: the row's name is the control that renames it, opening a sheet that arrives pre-filled; `PATCH /scenes/{id}` takes the name alone and leaves the actions untouched. Editing the actions of an existing scene is still a rebuild | **P1** |
| No scene reordering, and no "run from the dashboard" shortcut for a scene you use daily | P2 |

### Settings (`/settings`)
| Finding | Priority |
| --- | --- |
| Account, home name, time zone, location, devices, members, at-home, rooms, appearance, about | — |
| **No Notifications section** — *done, without quiet hours*: `GET /notifications/tokens` (count + platform, never the delivery address) and `DELETE /notifications/tokens`, surfaced as "Phones receiving alerts" and a confirmed "Turn off alerts everywhere". Quiet hours still needs server-side enforcement before it can be offered honestly | **P1** |
| **No Security section** — *sessions and second factor done on the server*: `GET /me/sessions` and `POST /auth/logout-others` are surfaced as "Signed-in devices" plus a confirmed, destructive "Sign out other devices"; `GET/POST/DELETE /me/totp` and `POST /me/totp/confirm` give the customer a real second factor (`second_factor_required` on sign-in, replayed codes refused, seed sealed with `HAVEN_VAULT_KEY`), and Settings → Security carries it: Off / Set up / On, the secret and `otpauth://` link as copyable text, a six-digit confirm, and a confirmed "Turn off". Sign-in asks for the code as a *step*, not an error. There is no QR: a QR needs a renderer this app does not carry, and the seed plus the link are what an authenticator actually needs | **P1** — done |
| Devices row is inert while Members navigates; two rows of the same family behave differently | **P1** — one line |
| About shows the version only: no licences, no support contact, no status link | P2 |
| No "leave home" here (it lives on Members) — the honest home of that action is ambiguous | P2 |

### Homes (`/homes`)
| Finding | Priority |
| --- | --- |
| Switch, create, per-home counts | — |
| **No rename, no delete, no leave** — *done*: each row has an Edit control opening a sheet (pre-filled name, owner-only delete behind a second confirmation that names the device count). Leaving lives on Members | done |
| **No per-home artwork** — *done*: each home draws its own hue from its id, stable across launches and spread across the four palette hues | done |

### Members (`/members`)
| Finding | Priority |
| --- | --- |
| Invite, role, remove, leave — all present, now with confirmation messages | done |
| Pending invitations are not listed, so an invite that was never accepted is invisible | **P1** |
| No "resend invitation" | P2 |

### Privacy (`/privacy`)
| Finding | Priority |
| --- | --- |
| Export and delete, with the exclusions named | — |
| **No progress or receipt for the export**: the download starts and nothing in the app records that it happened | P1 |
| Account deletion has no cooling-off state shown afterwards | P2 |

### Sign-in (`/login`)
| Finding | Priority |
| --- | --- |
| Sign in, create account, demo account, capped width | — |
| No "forgot password" and no password change anywhere in the app | **P1** |
| No passkey/Face ID entry point, which is the iOS expectation in 2026 | P2 |

---

## The pattern behind the list

Three kinds of gap, in order of how much they cost:

1. **A control that lies.** A row that looks editable and is not; an editor that opens blank; a toast that never came.
   These are worse than missing features because they cost the customer's confidence, and they are cheap to fix.
2. **A fact with no surface.** Serial, firmware, last run, pending invite, export receipt — the data is already in the
   API and nowhere on the screen. Each is a P1 because support cannot answer the question without the app.
3. **A section that should exist.** Notifications, Security, and the home's own lifecycle (rename, delete, leave).
   Bigger, and the order above is the order to build them.

## This pass

- `RoomRow` extracted to `pages/settings/RoomRow.tsx` with `RoomSheet`: pre-filled name, Save disabled until changed,
  destructive row separated from the primary action, delete confirmed outside the sheet so closing the sheet cannot take
  the question with it.
- `shared/ui/Toast.tsx` + `ToastProvider` in `app-shell/providers.tsx`; `useToast()` at nine mutation sites.
- Tests updated to the new interaction (`SettingsPage.test.tsx`), 26/26 across settings, members and device detail.
- Verified live: the sheet opened pre-filled with "Living Room", Save was muted until the name changed and accent
  afterwards, the row and the toast both read "Renamed to Living Room 2".

---

## This pass, part two — the P2 list

| Item | What it became |
| --- | --- |
| Alerts: dismiss | `DELETE /notifications/{id}` in `domain::notification::NotificationRepo`, a real socket test proving it removes one row and refuses another account's, and a `SwipeToDismiss` row whose threshold and direction rules are pure functions with tests. Screen readers get the same action through `accessibilityActions`. |
| Alerts: which home | Automations write `home_id` into the notification payload; the row prints the home's name **only when the account has more than one**, because a chip repeating the only home is a column of the same word. |
| Automations: conditions | "Only between two times" sends `Condition::TimeOfDay` — the second most common rule after sunset, and the engine has always accepted it. `clockValue` refuses `24:00`, `7` and `22:60` rather than sending a string that can never match. |
| Dashboard: long rooms | A room pages at eight tiles and ends with "Show N more in <room>". `buildRows` owns the rule, so it is proven by tests rather than by scrolling. |
| Home artwork | `homeAccent(id)` — FNV-1a over the uuid, four palette hues. A first-character hash fails on v7 uuids, whose first digits are a timestamp; that is why there is a test with four ids that differ only in their tails. |
| About | `Licences` (generated from the installed tree by `scripts/gen-licences.mjs`, 31 packages, all MIT) and `Copy diagnostics` — ids, counts, versions, timezone; never a token. |
| Demo data | `scripts/demo-fresh.sh` moves `last_seen_at`, `online` **and every point's `updated_at`**. The first version moved only the device row and the tiles still said "Silent for 1d": the app takes the *oldest* reading age across a device and its points, so the point's timestamp is the one that decides. |

Still blocked, and not faked: "identify" needs a data point the firmware answers, and remove has no undo because the server has no restore verb.
