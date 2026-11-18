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
| An all-offline home dims every field, so the *live* look (accent field, gauge) is nearly unreachable in the demo | P2 — demo data, not code |
| No "all devices" view: a room with 40 devices has no collapsed/expanded control | P2 |

### Device detail (`/device/[id]`)
| Finding | Priority |
| --- | --- |
| Rename, room move, ranges, chart, remove — present, and now with confirmation | done |
| **No identity block**: serial, product, vendor, firmware version and last-seen time are all in the payload and none are on screen. A support call cannot start with "what is this device" | **P1** |
| No "identify" action for a device you cannot see (blink, beep) — the protocol has no such command yet | P2, blocked on protocol |
| Remove is immediate after confirm, with no undo | P2 |

### Add a device (`/add-device`)
| Finding | Priority |
| --- | --- |
| Handoff explains itself, prints the link, offers copy — after this pass' fixes | done |
| The scan screen's nearby-device rows are plain rows; the radar animation is the only art | P2 |
| No "the device is not appearing" path (factory-reset instructions, AP-mode fallback) | **P1** — the most common first-run failure has no documented next step |

### Alerts (`/alerts`)
| Finding | Priority |
| --- | --- |
| Mark all read; opening one marks it read | — |
| **No filter** (all / unread) — *done*: an "Only unread" switch over the loaded feed, with "Nothing unread — you are caught up." for the empty case. Per-alert dismiss still needs an API verb | P1 → P2 |
| No empty state distinct from "no alerts at all" vs "all read" | P2 |
| Grouping is by day, which is right; nothing distinguishes homes for a person in two | P2 |

### Automations (`/automations`)
| Finding | Priority |
| --- | --- |
| Empty state, rule rows, run/history/delete, toggle — present | — |
| A rule row shows its schedule as text but has no "last ran at" in the row | **P1** — the first question about a rule that did not fire |
| No condition editor beyond presence/solar; `Condition` supports comparisons the UI never offers | P2 — documented gap |

### Scenes (`/scenes`)
| Finding | Priority |
| --- | --- |
| Create, run, delete; the form carries the accent on an empty screen | done |
| **No rename and no edit** of an existing scene: you delete and rebuild | **P1** |
| No scene reordering, and no "run from the dashboard" shortcut for a scene you use daily | P2 |

### Settings (`/settings`)
| Finding | Priority |
| --- | --- |
| Account, home name, time zone, location, devices, members, at-home, rooms, appearance, about | — |
| **No Notifications section**: quiet hours, and whether push is actually delivered on this device, are unanswerable in the app while the server sends them | **P1** |
| **No Security section**: active sessions, sign-out-elsewhere, TOTP — the API has the endpoints for vendor accounts, the consumer side has none | P1 — partly blocked |
| Devices row is inert while Members navigates; two rows of the same family behave differently | **P1** — one line |
| About shows the version only: no licences, no support contact, no status link | P2 |
| No "leave home" here (it lives on Members) — the honest home of that action is ambiguous | P2 |

### Homes (`/homes`)
| Finding | Priority |
| --- | --- |
| Switch, create, per-home counts | — |
| **No rename, no delete, no leave** — *done*: each row has an Edit control opening a sheet (pre-filled name, owner-only delete behind a second confirmation that names the device count). Leaving lives on Members | done |
| No per-home artwork: every home is a coloured initial | P2 |

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
