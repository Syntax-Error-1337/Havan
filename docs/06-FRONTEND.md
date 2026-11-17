# Frontend — feature-sliced, both clients

Two apps, one rulebook. The app is the product people judge; the portal is the
product vendors judge. Neither may become a pile of screens.

## Layers

```
app      composition root: providers, theme, navigation shell, error boundary
 └ pages     one per route; assembles widgets, owns loading/empty/error states
    └ widgets  a big self-contained block (radar, device card grid, scene canvas)
       └ features  a user action and its mutation (add-device, toggle-device, create-scene)
          └ entities  a domain object: model, api, hooks, ui (device, room, home, product)
             └ shared  ui kit, api client, lib, config, i18n
```

**Import only downward.** A `feature` may import `entities` and `shared`; it may
not import a `page`. Siblings may not import each other — a `feature` needing
another `feature` means the logic belongs in an `entity` or `shared`.

Enforced by `eslint-plugin-boundaries` with the layer order above; CI fails on a
violation. This is the single rule that keeps FSD from decaying into folders.

## Public API per slice

Every slice exposes exactly one barrel, and only what outsiders need:

```
entities/device/index.ts
  export { useDevices, useDevice } from './model/queries'
  export { DeviceCard } from './ui/DeviceCard'
  export type { Device, DevicePoint } from './model/types'
```

No deep imports (`entities/device/model/queries`) from outside the slice. Add an
`eslint no-restricted-imports` pattern for `*/model/*`, `*/api/*`, `*/lib/*`.

## The app (Expo)

```
app/                          expo-router routes — thin, no logic
  (auth)/login.tsx            → <LoginPage />
  (tabs)/index.tsx            → <DashboardPage />
  (tabs)/devices.tsx          → <DevicesPage />
  device/[id].tsx             → <DevicePage />
  add-device/index.tsx        → <RadarPage />
  scene/[id].tsx              → <ScenePage />

src/
  app-shell/      providers.tsx, theme.tsx, queryClient.ts, styles/
                  (NOT `src/app`: expo-router treats `src/app` as its route root
                   and would serve providers.tsx as a screen)
  pages/          login, dashboard, devices, device-detail, radar, scene-editor, settings
  widgets/        home-header, room-section, device-grid, radar-scanner,
                  device-control-panel, scene-canvas, activity-feed
  features/       add-device, control-device, rename-device, create-scene,
                  run-scene, invite-member, toggle-room, pair-wifi
  entities/       device, room, home, scene, product, user, notification
  shared/
    ui/           Box, Text, Button, Sheet, Slider, Toggle, Chip, Toast, Skeleton
    api/          generated client + interceptors + query keys
    lib/          result, time, format, permissions, ble, storage
    config/       env, features
    i18n/         en, de, hi, ar (rtl-aware)
```

Route files stay under 30 lines: parse params, render the page. A route that
contains a query is a bug. The auth gate lives in `app/_layout.tsx`, not in a
screen, so a new screen cannot forget it.

### Testing the app

There is no simulator here and no Xcode, so the screens are verified by
rendering them: `jest-expo` + `@testing-library/react-native`, with the network
stubbed at `globalThis.fetch`. `npm test`.

Setup gotchas, all learned the hard way:

- `jest-expo` needs `@react-native/jest-preset` installed explicitly — the RN
  preset moved out of it and it fails with a validation error until it is there.
- RNTL 14's `render` is **async**: `const view = await render(...)`. The global
  `screen` export is not populated by it, so use the returned `view`.
- RNTL 14's `fireEvent` is **async too**: `await fireEvent.press(...)`. Un-awaited,
  the press lands before the state update it depends on, the button is still
  disabled, nothing happens — and the assertion fails somewhere else entirely.
- Prefer `await view.findByText(...)` over `waitFor(...) + getByText(...)`; the
  latter races the query and passes or fails by timing.
- Query by placeholder and role, not accessibility label: `getByLabelText` did
  not match this `TextInput` even though the label was on the node.
- The suite needs `--forceExit` (the `npm test` script has it): something keeps
  a handle alive after the run and jest waits for it.
- Do **not** set `gcTime: 0` on a test `QueryClient` unless something observes
  the query. `setQueryData` with no observer is collected immediately, so a test
  that writes a cache entry and asserts on it passes alone and fails in a suite.
- `src/test/setup.ts` mocks the keychain, expo-router, safe-area and WebSocket.
  Add a mock there rather than in a test file, so no test can quietly reach a
  real network. `src/test/env.ts` sets `EXPO_PUBLIC_BLE_MOCK=1` before any import,
  because the radio is chosen at import time — the radar tests then drive the
  real stand-in radio rather than a mock of a mock.

These catch what typechecking cannot: the `{data}` bug (a bare array read as an
envelope) typechecked fine and left every home rendering "no devices yet".

### Testing the portal

`vitest` + `@testing-library/react`, jsdom, `npm test` in `portal/`. Eleven tests
over the three screens that matter: vendor login, the products table, and the
thing-model editor (load, invalid JSON, save, all-rules-at-once, published is
locked).

Two gotchas specific to this setup:

- **Do not add `plugins: [react()]` to `vitest.config.ts`.** The SWC plugin's
  types disagree with vitest's bundled vite and the config fails to typecheck;
  esbuild already handles TSX through the tsconfig's `jsx: react-jsx`.
- **`localStorage` is missing from `globalThis` in this jsdom.** Node 22 defines
  its own `localStorage` that is unavailable without `--localstorage-file`, and
  it shadows jsdom's. `src/test/setup.ts` installs a small in-memory Storage so
  the API client's module-scope read does not kill the suite before it starts.
- Adding `types` to `tsconfig.json` replaces the default; keep `vite/client` in
  the list or `import.meta.env` stops typechecking.
- `userEvent.type` treats `{` and `[` as key descriptors — use `fireEvent.change`
  when the text being typed is JSON.

### The radar, as built

`shared/lib/ble` is the only place that knows the wire format, and it is split
so the interesting half can be tested without a radio:

| file | does |
|---|---|
| `protocol.ts` | pure: decode the advertisement, chunk a JSON message, map RSSI to bars |
| `radio.ts` | the `Radio` interface the screens use |
| `plx-radio.ts` | `react-native-ble-plx`: scan, connect, read Info, write Rx, watch Tx |
| `mock-radio.ts` | advertises the seeded products and drifts their signal |

`EXPO_PUBLIC_BLE_MOCK=1` swaps in the mock at import time, in `index.ts` and
nowhere else. The pairing screens are then developed and reviewed on a laptop;
they run the same state machine the phone runs.

Three decisions worth knowing:

- **The ticket comes from the phone, not the device.** `usePairDevice` mints one
  through `/homes/{id}/pairing-tickets` and writes it over BLE with the Wi-Fi
  credentials and the device's nonce. The device holds the secret; the ticket is
  just permission to claim once.
- **The result frame is authoritative, polling is the fallback.** The device
  sends `{"t":"result"}` on Tx when it has claimed itself; `plx-radio` waits up
  to 25 s for it. If it never arrives the device is still joining, so the screen
  reports success and watches `/devices` for a minute before saying it failed.
- **Ads are folded, not listed.** A device re-advertises several times a second;
  packets go into a ref keyed by `productHash:serialTail` and are flushed on a
  timer, so the list re-renders twice a second instead of forty times.

### The home's clock, and the shortcut under it

Settings showed `Time zone — UTC` as a **read-only row**. Every home created before the first-run screen asked for
a name was created in UTC, and once created it could never be moved: a customer who moves, or who was set up
wrongly, keeps a home whose schedules fire at the wrong hour forever. It is now an editable row with a one-tap
*Use Asia/Taipei* (the phone's own zone) beside it.

Fixing that exposed the shortcut it was built on:

```rust
/// shortcut: shape only, not the tz database — "Mars/Olympus" passes. Upgrade to a real lookup when
/// schedules exist (Phase 6), because a wrong zone shifts every automation silently.
fn valid_timezone(tz: &str) -> bool
```

**Schedules exist.** Phase 6 shipped; `crates/domain/src/homes.rs` was still accepting `Mars/Olympus` by
counting slashes. The due upgrade needs no dependency: Postgres ships the authority as `pg_timezone_names`, so the
repo asks the database whether the zone exists, and the service refuses anything it does not know.

Verified against the running API:

| Request | Result |
|---|---|
| `PATCH {"timezone":"Asia/Tokyo"}` | **200**, home returns `Asia/Tokyo` |
| `PATCH {"timezone":"Mars/Olympus"}` | **422** `that is not a timezone this platform knows…` |
| the demo's `Workshop`, before this pass | `UTC` — the exact case this fixes |

And in the app: the row reads `Time zone — Asia/Taipei`, offers *Use Asia/Taipei* when the home is somewhere else,
and tapping it moved the home from `Europe/Berlin` back — confirmed in the database, not just on screen.

### A palette nobody could reach

`providers.tsx` carried this comment:

> *The light palette is still maintained and still tested — Settings can switch to it.*

There was no such control. The light half of the palette had been contrast-gated, screenshot-reviewed and fixed
(opacity, muted weight, `accentInk`) in two earlier passes, and **no customer could see it**: `ThemeProvider`
took its mode from the operator's theme and nothing else. A comment that describes a feature that does not exist
is worse than the missing feature, because it stops the next person looking.

There is now an **Appearance** section in Settings — *Match my phone · Light · Dark* — backed by
`entities/preferences`, a small store persisted beside the session (keychain on a phone, `localStorage` on the
web) because it is this device's opinion, not the account's. `PlatformThemeProvider` reads it and the customer's
choice wins over the operator's mode; `system` (the default) keeps the operator's answer. The comment now says
what is true.

Verified live: tapping **Light** repaints the whole app — cards, text, chips — writes
`haven.appearance = light`, and survives a reload; switching back to *Match my phone* writes `system` and returns
to the operator's dark brand.

### An empty home said "Try again"

A new account that had just named its home landed on:

> **No devices yet** — *Tap Add and we will look for devices nearby.* — **Try again**

Three defects in one small block. `Try again` retried a request that had **succeeded**: the empty home and the
failed home had been separated last week, and the retry stayed behind in the wrong branch. "Tap Add" pointed at a
`+` the copy never described. And the only guidance on offer was one grey line.

The empty home is the first screen a paying customer sees, so it now says what to do **in the order people
actually do it**:

1. **Power the device on** — plug in or batteries; it makes itself findable in a few seconds.
2. **Pair it from your phone** — *tap “Pair a device” below*; Bluetooth on, permission granted.
3. **Give it a room** — *optional, and changeable later*.

The order was wrong in the first draft of this screen too: it led with *Add a room*, which is administrative work
for somebody who wants their new bulb to turn on. Rooms are how you find a device **afterwards**, and the list
says so.

Three things went with it, all found by looking at the screenshot rather than the code:

- **The presence pill is gone on an empty home.** `no devices` sat beside `0 devices · 0 rooms` and the heading
  `Flat 3 is empty` — the same fact three times, and a pill is the most expensive of the three.
- **Its green dot was a lie.** The dot was green whenever `silent === 0`, and zero devices means zero silent
  devices: an empty home reported as healthy. It is muted when there is nothing to report.
- **The search box and category chips are hidden** when there are no devices. A control that can only fail is
  not a feature.

Empty home **6 → 9/10**.

### The first sixty seconds of a new account

Signing up with a fresh address lands on **`Create your first home`** with one button, which posts:

```ts
createHome.mutate({ name: 'My home', timezone: 'UTC' })
```

Two things wrong with one line. The name is one nobody chose — and it is not a label, it is the thing the
customer will say out loud for years ("Flat 3", "The cottage"). And the timezone is **UTC for every person on
the planet**, which is not cosmetic: schedules and automations are evaluated in the home's zone, so a customer in
Taipei gets their 7 a.m. automation at half past two in the afternoon.

The screen now asks for the name (defaulting to `My home` if they skip it) and sends `deviceTimeZone()`, which
reads the IANA zone from `expo-localization` and falls back to UTC rather than to a guess — a wrong zone is a
schedule firing at the wrong hour. Verified live: a brand-new account created **`Flat 3` / `Asia/Taipei`**, where
the same request used to produce `My home` / `UTC`.

Two smaller things the first-run screen was missing, both about a person who is already stuck:

- **A way out.** Signed in on the wrong account, there was no home and no exit — the same rule as every other
  state in this document. There is now *Use a different account*.
- **The invitee's sentence.** Someone who has been added to a partner's home sees this screen and reasonably
  concludes the app is broken. It says: *"Has someone added you to their home? It will appear here once they
  do."*

The tests for this needed two fixes of their own, both worth recording: `findByLabelText` on a labelled wrapper
returns something `changeText` cannot type into (use a testID on the input), and a press that follows a
`changeText` can run before the state commit — assert the field's value first, then press.

### A pill that counted the page

The header of a 1,200-device home read **`200 offline`**. Two hundred was the size of the dashboard page it had
fetched; the home's own answer was *1,200* — every one of the seeded devices had long stopped reporting. The
presence pill was summarising our pagination and printing it as a fact about the house.

This is the third appearance of the same rule in this document ("a count is a fact about the home, not about the
page you fetched"), and the first two were fixed by *reading* `devices_total` rather than by counting client-side.
Here the number did not exist at all: the dashboard answer carried the page, the home's total, and nothing about
presence.

So the server counts it. `devices.home_presence` is one aggregate over the home's rows:

```sql
SELECT count(*),
       count(*) FILTER (WHERE d.online),
       count(*) FILTER (WHERE NOT d.online
                           OR d.last_seen_at IS NULL
                           OR d.last_seen_at < now() - GREATEST(
                                  interval '15 minutes',
                                  COALESCE(d.report_interval_secs, 60) * 3 * interval '1 second'))
  FROM devices d WHERE d.home_id = $1
```

That is the client's own silence rule — `max(15 min, 3 × report_interval)` — written once on the server so the
header and the tiles cannot drift apart. `GET /homes/{id}/dashboard` now returns `presence: { total, online,
silent }`, and the app prefers it, falling back to the page summary when talking to an older server.

Verified against the running API and then on the Plant home: `presence: { total: 1200, online: 0, silent: 1200 }`,
and the pill reads **`1,200 offline`** instead of `200 offline` (with locale separators, like the subtitle above
it — "1200 offline" is a number, "1,200 offline" is a count).

### A fleet you cannot look anything up in

The vendor's fleet screen showed 1,909 devices, 200 at a time, with one filter: a `State` dropdown. The first
thing support does with a fleet is **find one device** — a serial a customer reads off the label — and there was
no way to do that. The choices on offer were paging, or pressing *Load all 1,909* and using the browser's find.

Search is server-side, because paging is: `q` is a substring of the serial or the name, trimmed, capped at 64
characters, with `%` and `_` escaped so that a typed wildcard is a character rather than the whole fleet. Without
an index, `ILIKE '%term%'` is a sequential scan of every device on the platform — fine at 1,909 and unusable at
the millions this platform claims — so migration `0041` adds `pg_trgm` (PostgreSQL contrib, already permitted by
ADR-002) and GIN indexes on `devices.serial` and `devices.name`.

Verified against the running API and then in the browser:

| Typed | API | Screen |
|---|---|---|
| `HUGE-01200` | 1 row | `1 of 1,909 devices matching “HUGE-01200” shown`, one table row |
| `lamp 12` | 5 rows (by name) | — |
| `%` | **0 rows** (escaped, not a wildcard) | — |
| `zzzz` | 0 rows | *No device matches “zzzz”* + *Clear the filters* |

The empty state distinguishes "nothing matches this search" from "this vendor has no devices", which is the same
mistake the dashboard made in the other direction.

### An intermittent gate is a gate nobody trusts

`make check` went red on `-p api --test webhooks` three times in one afternoon, on two different tests
(`the_outbox_delivers_once_then_stops_being_due`, then `an_event_reaches_every_endpoint_that_asked_for_it`), and
passed when run alone. The suite already had a mutex and a comment explaining that `due()` is a global queue —
so the race had been found once and only half-fixed.

The half that was missing was arithmetic, not locking: **`due()` claims any vendor's rows, oldest first, up to a
limit**, and the test database accumulates undelivered rows from every suite that has ever run against it —
552 undelivered, 143 due, measured. `due(500)` therefore returned the backlog and stopped *before* the two rows
the test had just inserted, so the assertion failed with `left: 0, right: 2` on a queue that was working
correctly.

The fix is one constant: the delivery tests ask for `ALL_DUE = 10_000`. An assertion about delivery has to ask
for everything, or it is an assertion about the queue's backlog. Three consecutive runs at four threads: 6/6.

The general lesson is the one this project keeps re-learning: a test that shares a database has to say what it
owns. The mutex serialises *this* file; it cannot know that a limit of 500 was quietly cutting the file's own
rows out of its own assertion.

### An invoice a vendor could not check

`/invoices` rendered 169 rows and one total. On the seeded month the first ten rows read **0.00 USD** — the
devices activated against a free grant — so the document opened on a column of zeroes and closed on `44.82 USD`,
with nothing between them. A vendor reading that has no way to tell a free activation from a device that failed
to bill, and no way to check our arithmetic without exporting and summing 169 cells.

The data was right the whole time (`18 × 2.49 = 44.82`, the rest at `0`); the *screen* never said so. Three
changes, all in the document rather than in the query:

- **A reconciliation line above the table**, computed from the rows themselves:
  *18 devices at 2.49 USD = 44.82 USD · 151 devices at no charge · total 44.82 USD.*
- **`No charge` instead of `0.00 USD`** on a line whose price is zero. A zero that is a decision and a zero that
  is a failure look identical in a money column, and only one of them is the vendor's problem.
- **A footer row** with the quantity and the total, so the table closes on the same number as the header.

Verified in the browser: the strip reads `18 devices at 2.49 USD = 44.82 USD · 151 devices at no charge · total
44.82 USD`, the footer says `Total · 169 devices 44.82 USD`, and the first cell now says `No charge`.

## Portal (React)

### A login form that asked for a primary key

The portal's door asked for a **Vendor ID**: thirty-six characters, dashes, no meaning — with a `pattern` that
refused anything shorter. It is the one credential in this product that a person cannot remember, check, or read
out to a colleague, and it was the *first* field on the screen.

The `vendors` table has carried a `slug` since the first migration (`acme` — the short name an operator writes in
their own notes). The door now accepts either: `IdentityService::resolve_vendor` parses a UUID, otherwise looks
the slug up, and the form asks for a **Workspace**.

The failure mode is deliberately dull: an unknown workspace and a wrong password are the same
`InvalidCredentials`. Naming "no such workspace" would let anyone enumerate the platform's customers, so the
resolution happens before the password check and returns the same error.

Verified against the running API:

| Body | Result |
|---|---|
| `{"vendor":"acme", …}` | **200** |
| `{"vendor_id":"11111111-…", …}` (older client, serde alias) | **200** |
| `{"vendor":"nope", …}` | **401** |

### The money line that read as an overcharge

`/licences` said, under a sentence explaining that a licence is charged **when a device is activated**:

> *498.00 USD authorised at 2.49 USD per device.*

498.00 is 200 × 2.49 — the whole grant. The page also showed **185 activated**, so a vendor doing the arithmetic
the sentence invites gets 460.65, and a page that prints 498 next to 185 reads as a 37.35 overcharge. Both numbers
were true; the page never said which was which.

It now reconciles all three, in one sentence:

> *Granted value 498.00 USD (200 × 2.49 USD) · consumed by 185 activations 460.65 USD · unused 37.35 USD.*

A money screen's job is not to be honest in aggregate, it is to be unambiguous line by line — and "authorised"
was doing work nobody could see.


### The vendor book described the test suite

The operator console opened on **55 vendors**. Thirty-six of them were `E2E Vendor` and `To Suspend`, one per
admin end-to-end run, each with a `e2e-<timestamp>` slug. The header said "55 vendors · 1 suspended" — a
number about the suite, in the place where the operator reads how big the business is.

The two specs onboarded with a fresh timestamp every execution. They now onboard `e2e-onboard` and
`e2e-suspend` — fixed slugs, so a second run finds the row it made and onboarding is a no-op — and name them
`Test vendor (e2e)` / `Test vendor (suspend)`, because an operator scanning the book should not have to guess
which rows came from a test. Deleting the debris took the count from 55 to 19 real vendors, and two further
suite runs moved it by zero.

**A list is not a console.** The vendors table had no search, no filter, and two columns that named neither of
the ceilings they showed: `Licences 0 / 0` is *activated of granted*, and `Quota 50` refuses the **next**
activation even when licences remain. Now: search over name and slug (a support call starts with one or the
other, or with a line from a log), `All / Active / Suspended` chips, an explicit empty state, and headers
`Activated` and `Device cap` with the definitions in their tooltips.

Operator console **6 → 8/10**.

### Test data is data

The vendor portal's first screen, on the seeded account, was **eighteen products named `E2E Bulb`**, each
`draft` / `uncertified`, each with a red `Delete` button and nothing else. Scored **4/10** as a first
impression — and it was about the *seed*, not the layout.

The rows were real: `e2e/02-products.spec.ts` creates a product with a unique key on every run and cleaned up
nothing. The file said so out loud — *"a draft product is harmless"* — and after eighteen runs the product list
was a log of the suite's own executions. Test data is data: it is what the next person sees, it is what a
screenshot in a sales deck shows, and a vendor's first ten minutes are the demo of the whole platform.

The spec now deletes what it created (verified: eight tests, three products afterwards), and the debris was
cleared — 14 `E2E Bulb` rows and one product called `ok` under the key `okok`. **Portal 4 → 8/10** with three
real products on the page.

### A table of things to remove

Every row offered exactly one action, `Delete`, drawn as a filled red button — so a list of products read as a
list of things to delete. Each row now reads `Open` then `Delete`, with the delete as a quiet red text button
(`.btn--danger-quiet`) and the name as a link with a visible sibling that says so.

The table grew the two controls a vendor with two hundred products needs: a search over **name and product
key** (what is written on the box, and what is written in the firmware) and `All / Draft / Published` chips,
because "what is not published yet" is the question asked before every demo. The count appears only when
something is hidden (`1 of 3`) — `3 shown` beside `3 products` is one fact twice.

```
src/
  app/            router, providers, layout, theme
  pages/          products, product-editor, firmware, fleet, campaign, analytics, settings
  widgets/        product-table, thing-model-editor, datapoint-mapper,
                  fleet-map, rollout-chart, api-key-panel
  features/       publish-product, upload-firmware, start-campaign, rotate-key
  entities/       product, firmware, device, vendor, campaign
  shared/         ui, api, lib, config
```

Same layers, same lint. The thing-model editor is the one genuinely complex
widget: it is a form over a JSON schema with live validation against the
capability catalog — validate locally with the same rules as the server (shared
Zod schema generated from the OpenAPI document) so the vendor sees the error
before the round trip.

## Design system

One token file per app, consumed by primitives. No component reads a raw hex.

```ts
// shared/config/tokens.ts
export const tokens = {
  color: {
    bg:        '#0B0D10',  surface: '#14181D',  elevated: '#1B2027',
    border:    '#252C35',  text: '#F2F5F8',     muted: '#8A97A6',
    accent:    '#4C8DFF',  success: '#2FBF71',   warning: '#F2A33C', danger: '#E5484D',
  },
  radius: { sm: 8, md: 12, lg: 18, pill: 999 },
  space:  { 1: 4, 2: 8, 3: 12, 4: 16, 6: 24, 8: 32 },
  font:   { body: 15, title: 20, display: 28 },
  motion: { fast: 140, base: 220, slow: 380 },
}
```

Both clients ship **dark first** (that is what smart-home apps look like at
night, and it is what customers screenshot), with a light theme from the same
tokens. White-label vendors override `color.accent`, `color.bg` and the logo
from `/vendor/branding` at build time and at runtime.

Styling: `StyleSheet.create` + `useTheme()` on mobile — **not** nativewind or
gluestack (both break on the RN version we target; the FlatList wrapper throws
on `react-native-css`). Tailwind 4 on the portal, where it is safe.

## Data layer rules

| Rule | Why |
|---|---|
| All server state through TanStack Query | one cache, one retry policy, one place to inspect |
| Query keys are tuples from a single factory (`qk.device.list(homeId)`) | no typo'd cache keys, easy invalidation |
| Mutations are optimistic only where the device acks fast (< 2 s) | a lying toggle is worse than a spinner |
| WebSocket events write into the query cache (`setQueryData`) | no refetch storms |
| Offline: persist the cache (MMKV) and queue commands | a home without internet still shows state |
| Generated API client, never hand-rolled fetch | types come from OpenAPI |

Command UX: tap → optimistic `pending` value in the cache → `202` → `settled`
event replaces it. If the command settles `rejected`, revert **and** show the
device's own message ("brightness must be 1..100"), not "something went wrong".

## Performance budgets

| Metric | Budget |
|---|---|
| Cold start to dashboard painted | < 1.5 s on a mid-range Android |
| Radar first result | < 1.2 s after scan start |
| Device toggle feedback | < 100 ms (optimistic) |
| Dashboard with 120 devices | 60 fps scroll, no dropped frames |
| JS bundle, initial | < 3.5 MB (Hermes bytecode) |
| Portal LCP | < 1.8 s on cable |

Techniques that are mandatory, not aspirational: `FlashList` for device grids,
`React.memo` on `DeviceCard`, selector-based Zustand stores, `reanimated` for
the radar (no `setState` per frame), image `expo-image` with `blurhash`, and
route-level code splitting on the portal.

## Accessibility (shipped, not deferred)

- Every control has a label and a role; toggles announce state.
- 44×44 minimum touch target; the radar's cards are 64 tall.
- Dynamic type up to 200 % must not clip; no fixed-height text containers.
- Colour is never the only signal (online state has an icon).
- Reduce-motion honoured: the radar becomes a progress bar.

## Testing

- `shared` and `entities`: pure unit tests (vitest on portal, jest on app).
- `features`: one test per mutation — success, server error, offline.
- `pages`: render test with MSW handlers, asserting loading → loaded → error.
- E2E: Detox on the app for onboarding + pairing + toggle; Playwright on the
  portal for publish-product and start-campaign.

`renderScreen()` helper wraps providers once so a page test is three lines.

### Looking at a screen without a simulator

`make app-web` serves the customer app at `:8082`. It exists because there is no
Xcode on the machine this was built on, and because the cheapest way to catch a
broken layout is to look at one.

Two things had to change for it to be usable, and both were bugs rather than
configuration:

- `expo-secure-store` throws on web instead of degrading, and the app hydrated
  the session eagerly — so the web build was a **blank page**. `shared/lib/secure.ts`
  now owns the choice: keychain on a phone, `localStorage` on web. One module
  knows; the rest of the app asks it.
- the API sends no CORS headers, so a browser client could not call it. The dev
  build now allows any origin (behind `cfg!(debug_assertions)`); a release build
  emits none. A native app sends no `Origin`, so this costs production nothing,
  but a deployment that wants a browser client must set its own origin.

What the browser showed that no test did: the dashboard header read **"0
devices"** above a list of one. `homes.device_count` is a `count(*)` over
`devices`, which carries an RLS policy keyed on `haven_user()` — and the homes
query never set it, so the count was not *wrong*, it was zero by policy. The
homes repo now opens the transaction with the caller's identity.

### Checking the build, not the dev server

`npm run build && npx vite preview` serves the bundle that would actually ship.
`vite preview` does **not** inherit `server.proxy`, so `vite.config.ts` declares the
same `/v1` proxy under `preview` — otherwise the only way to exercise a build was to
run the dev server, which is not the same code.

Driving that bundle turned up a bug the dev server hid: with an expired token the
portal rendered the signed-in shell and printed the API's message
("missing bearer token"). The client cleared the token on 401, but clearing
localStorage does not re-render a React context. `onTokenChange` now lets the
session provider follow, so an expired session lands on the login screen.

### Tests on a cold runner

`npm test` is what CI runs, and CI starts from a fresh `npm ci` with an empty
transform cache — several times slower than a warm laptop. Verified by doing it:
`git archive` the app to a temp directory, `npm ci --legacy-peer-deps`,
`expo export`, `tsc`, then `jest` with an empty cache directory. The first cold run
failed six tests on the default 5 s timeout that pass in a second locally, so
`jest.config.js` sets 15 s with that reason written down. A timeout should mean
"this test hung", not "this machine is cold".

## Screens

| Screen | Reads | Notes |
|---|---|---|
| Home | `/homes/{id}/dashboard` | one request; the payload seeds the device, room, scene and unread caches so widgets keep their own hooks without a second round trip. The socket patches both the dashboard and the device caches |
| Device | `/devices/{id}`, `/history`, `/updates` | bool → switch, numeric → stepper with the point's own min/max |
| Scenes | `/homes/{id}/scenes` | says **Sent**, never **Done** — dispatch is not settlement |
| Alerts | `/notifications` | unread count is its own endpoint |
| Settings | `/me`, `/homes`, `/homes/{id}/dashboard`, `/homes/{id}/rooms` | grouped rows; each editable row owns its draft so the Save of one row cannot send another's text |

## Design language

### An empty house and an unanswered question

Blocking the dashboard request at the network layer and reloading — the way a customer experiences a train
tunnel — the app said:

> **Home** · *no devices* · *0 devices · 0 rooms* · **No devices yet** — *Tap Add and we will look for devices
> nearby.*

The home has six devices and four rooms. Every word of that is a claim the app had no evidence for, and the last
line invites someone to re-pair hardware that is already on the wall. `Try again` was underneath it, which only
proves the code knew it had failed — the copy did not.

The empty state had quietly become the failure state, and it was not only this screen. `QueryError` in
`shared/ui` is now the one implementation, used by the dashboard, `/scenes` and `/alerts`, with three sentences
doing the work: **what could not be loaded**, **that nothing has changed**, and **a way to ask again**. An error
screen without a retry is a dead end with an explanation.

The counts had to change too. `0 devices · 0 rooms` is not "unknown"; the header now prints `— devices · — rooms`
and the presence pill says `unknown`, which is the same em-dash rule this document already applies while a count
is in flight — a rule that turned out to matter more on the failure path than on the loading one.

Three things were verified by blocking a real request and reading the screen:

| Screen | Before | After |
|---|---|---|
| `/dashboard` | *No devices yet — Tap Add…* | *We could not load this home … Your devices are still there* |
| `/scenes` | *No scenes yet. Build one below.* + a Save that cannot work | *We could not load your scenes* |
| `/alerts` | *Nothing new* above *Alerts are unavailable right now* | *— unread* + *We could not load your alerts* |

And the recovery was verified by unblocking and pressing `Try again`: the dashboard came back with `Workshop ·
6 devices · 4 rooms`, and the request log shows the failing `0`-byte attempts followed by a `200`.

### The first frame was blank

Throttled to Slow 3G and reloaded the app the way a customer does, the screen was a **blank white page for two
seconds, then four**, and only then the dashboard. Not a spinner, not a skeleton, not the brand — nothing. A
blank screen is indistinguishable from a broken app, and in a white-label product it is the operator's logo
replaced by the browser's default background.

The cause is not a missing loading state; it is that **no code of ours runs before the bundle does**, and the
bundle is what is slow. Three things were done about it, and one was documented instead:

- **`expo-splash-screen` is now a dependency** (MIT, `app.json` plugin, `#0B0D10`, the app icon at 120 pt). It was
  simply absent, which means the native first frame was uncovered too.
- **The splash is held until the session is known.** `preventAutoHideAsync()` in the root and `hideAsync()` after
  keychain hydration: without it the OS splash drops at the first frame, so a signed-in customer watches the
  *login form* appear and vanish on every cold start — the app looks like it signed them out.
- **The web fallback is untouched by that fix**, and this is the honest limitation: in Expo's default `web.output:
  'single'` the HTML is a fixture the CLI owns, so `app/+html.tsx` and the splash plugin's web injection are both
  ignored — verified by exporting and grepping: `splash` appears **zero** times in the built `index.html`. A
  branded pre-bundle canvas needs `output: 'static'`, which is a routing and deployment decision, not a coat of
  paint, so it was not taken here. `+html.tsx` was written, proven inert, and deleted rather than left in the
  tree looking like it does something.

### A confirmation form that cannot succeed is not a confirmation

The privacy screen scored well on separation — the dangerous card is red, bordered, and below the safe one — and
badly on one thing: it offered a button the server would refuse.

`erase` answers **409** when the person still owns a home that somebody else lives in, because deleting the
account would either evict them or leave them with a home whose owner no longer exists. That is the correct
server behaviour, and it is also the refusal a real customer hits. The screen now reads the homes list it
already has, and when a shared owned home exists it **turns the button off and says why**:

> *You still own Workshop, and somebody else lives in it. Deleting your account would take their home away, so
> it is refused until the home is handed over or they have left it.*
> *Nothing has been deleted. The button is off for the reason above, not because it failed.*

The last line matters more than it looks: a disabled control with no explanation reads as a bug, and a person who
thinks the app is broken tries again rather than going to fix the actual blocker.

Two smaller additions to the destructive card, both facts a person needs *before* the confirm sheet rather than
after it:

- **What is kept.** `DELETE /me` tombstones the person and keeps a receipt: *"A receipt of the request is kept
  for the retention window and then purged. It records that the account was erased; it holds none of its
  readings."*
- **What happens to other people.** The card already said homes you are a *member* of stay; it now also covers
  the inverse — an owned home takes its members' access with it, which is exactly the reason for the 409 above.

Verified live for the non-blocked path (the demo account owns nothing shared, so the button stays live), and by
test for the blocked one — including that no `DELETE` is ever sent when it is off.

### A bar chart was answering the wrong question

The device screen had its own chart — a bar sparkline — and `heights()` scaled every series from **zero**:

```ts
const min = isBoolean ? 0 : Math.min(0, ...values);   // the zero belongs to booleans only
```

A day that moved between 16.6 and 21.5 °C therefore drew twenty-six bars of 0.77, 0.80, 0.83 — a rectangle. The
screen's one job is "what has it been doing", and the honest answer from that picture was "nothing". It had a
test asserting the behaviour ("18° and 21° must look close together"), which is a fine rule for a count and a
bad one for a sensor.

Two things were wrong at once, so both were fixed:

- **The encoding.** Bars measure magnitude, so a bar chart *must* start at zero. A line describes change and may
  show a band. This screen has always been a time series; it now uses `AreaChart`, the same chart the dashboard
  hero uses, with the same gap handling and the same truncated axis.
- **The declaration.** The range is printed under the chart (`16.6–21.5 °C over the last day`) because the axis
  no longer includes zero. A truncated axis that says so is honest; a full axis that flattens the data is not.

`Sparkline`, `heights()` and its tests are deleted rather than left unused — one chart component, one scale rule.

Two more defects from the same review:

- **The room chooser sat between the offline banner and the readings.** The first thing under "this device has
  stopped reporting" was a list of rooms; it is now at the bottom, labelled `In which room`, above the
  destructive action. Configuration comes last.
- **Unselected rooms looked disabled** (`glass` fill, muted label) — the same defect the scene editor's chips had.
  They are `surface` + full-strength text now, in one horizontally scrolling row instead of a wrap that left
  `Kitchen` alone on a second line.

And one the probe found by accident: **the navigator's card background was the static palette's**, not the
operator's (`contentStyle: { backgroundColor: tokens.color.bg }` at module load). It is read from the runtime
theme now, so the gaps between screens follow a theme change instead of keeping the old colour.

Device detail **6 → 9/10**.

### Rendered contrast is the only contrast that counts

`check-contrast.mjs` compares tokens against a base background. It cannot see a translucent card, a mesh
gradient, or a compositing effect — and all three sat between the token and the customer's eye. This pass
measured instead: a screenshot at devicePixelRatio 2, the bounding box of every leaf text node from the DOM, and
per box the modal pixel colour as the surface with the extreme glyph core as the ink.

The method costs about ten lines of probing and found things the gate structurally could not:

| Element | Before | After text-weight fix |
|---|---|---|
| 11 pt secondary caption over a card | 3.85:1 | 4.78:1 |
| banner text over the mesh | 4.27:1 | 5.53:1 |
| `now` on the chart axis | 4.21:1 | 5.49:1 |
| accent text at 11 pt | 4.08:1 | 6.5–8:1 |

Two rules came out of it, and both are now in the palette rather than in a component:

- **Dark `muted` is 0.72, not Apple's 0.60.** At 60 % white it is 5.4:1 on the base background — passes the gate —
  and 3.8–4.4:1 on the card it is actually drawn on.
- **`accent` is a fill; `accentInk` is text.** `#146EF5` is a fine button and a 4.08:1 label at 11 pt. `accentInk`
  is the same hue two steps lighter (`#5AA2FF`, 7.17:1), and `tone="accent"` in `AppText` now resolves to it —
  a sentence is not a glyph. The gate enforces 4.5 on `accentInk` and 3 on `accent`, which is exactly the
  distinction.

The probe also produced two false positives (a 13-point label whose box excluded its glyphs, and a badge whose
surface *was* its text), which is a fair summary of the technique: useful, and not to be trusted without looking
at the picture.

### Light mode was never looked at

Every screenshot in this document is dark, and the palette has always had a light half. Flipping the operator
theme to `light` (one `PUT /v1/ops/theme`) found the app mostly correct — the token discipline paid for itself —
with **one class of defect no token can fix**:

> An 11-point footer caption in the hero card rendered at **3.49:1**. Measured from the screenshot's own pixels,
> not from the palette: the text colour was fine, the surface was fine, and the card's `opacity: 0.88` — the
> "this is stale" treatment — multiplied every child's alpha, including the text's.

**Opacity is not a safe way to express state.** `opacity` on a container is a compositor instruction: it drags
images, borders and *type* down together, so a card that is 12 % quieter to a designer is 12 % closer to failing
AA for a customer. The stale treatments on the hero card and on the device tiles now work per element — icon and
chart fade, the halo goes, the dot changes colour, the words say `last reading` — and the text keeps the token
the contrast gate already certified. Re-measured from the same pixels: **6.34:1**.

Two things this pass settled for free, because a light palette is where hardcoded colours show up:

- The mesh, header, presence pill, stat cards, chart and tab bar render correctly in light mode with no
  per-component overrides — the point of `tokens.ts` being the only file allowed a hex literal.
- `check-contrast.mjs` cannot catch this class of bug. It compares tokens against a base background; the defect
  was a compositing effect *above* the tokens. Rendered pixels are the only honest measurement, which is why the
  number above came from an image rather than from the palette.

The app speaks iOS, not "a dark theme". `src/shared/config/tokens.ts` is the whole
system, and the numbers in it come from Apple's Human Interface Guidelines rather
than from taste:

- **Type** — the SF text styles at default Dynamic Type: largeTitle 34/41,
  title2 22/28, body 17/22, footnote 13/18, caption2 11/13. Every `AppText` sets a
  size *and* its leading, because a 17 pt body with the wrong line height is what
  makes text look "off" on iOS. `allowFontScaling` stays on: a person who raised
  the system text size gets larger text and the layout copes.
- **Colour** — one palette per appearance, and **night is the default**: the app
  ships dark unless a real preference says otherwise, because a home interface is
  used in the evening next to a lamp it just turned on. The canvas is a two-stop
  gradient (`bgTop → bgBottom`, `#0B1020 → #05070F`) rather than a flat black, so a
  translucent card reads as glass; every surface above it is `glass`
  (`rgba(255,255,255,0.06)`) rather than a solid grey. The light palette is still
  maintained and still tested — it is a second appearance, not a leftover.
- **Category colour** — a product declares a `category`, and that alone decides its
  accent: bulbs violet, locks pink, cameras blue, climate orange, sensors cyan
  (`categoryAccent` in `tokens.ts`). An unknown category falls back to the theme
  accent, never to grey, because a new kind of device must not be the one that looks
  broken. The same accent tints the icon halo, the border, the control and the chart
  fill, which is what makes a vendor's product look designed without the vendor
  designing anything.
- **The canvas is light, not a colour** — `MeshBackground` draws three radial washes
  (teal high left, violet right, indigo at the foot) over a near-black base. A single
  vertical gradient reads as a coloured rectangle; three overlapping washes read as a
  room lit from three sides, which is what makes the translucent cards look like glass.
  The bloom positions are fixed, so a repaint never looks like the mood moved.
- **One hero, then the grid** — the home screen leads with the reading that matters
  (first online device with a numeric point, preferring temperature), shown big with
  its last 24 hours under it. Equal tiles make a person read every tile to find the
  number they opened the app for. The range printed under the chart includes the live
  reading, because a card whose maximum is below the number above it is a card nobody
  trusts twice.
- **Chrome floats** — `TabBar` sits above the content with a scrim fade, not docked,
  so the card language above it stays one surface. The centre action is raised: adding
  a device is what a person opens the app to do on day one.
- **Layout** — 4 pt grid, 16 pt margins, and a **44 pt minimum hit target**, which
  is the smallest a finger hits reliably.
- **Structure** — `Section` + `Row` is still the shape for settings (grouped list,
  hairlines inset to the text, one trailing control). Devices are **tiles**: a
  two-column grid inside a room, with `Glass` drawing the halo as a second offset
  layer rather than a shadow, so iOS, Android and web render the same card. A room
  holding a single device gives it the full row — a half-width card alone reads as a
  tile that failed to load.
- **Controls come from data** — `controlFor(point)` reads the thing model and picks
  the control: `bool` → switch, bounded number → slider, temperature → stepper,
  read-only → readout, with `code` as the address on the wire. There is no
  per-product code path, so a product shipped tomorrow already has controls today.
  `brightness.level` is a slider because it is bounded; `color_temp.kelvin` is a
  stepper because a person says "2700", not "2694".
- **State** — a control that cannot act is disabled and visibly so: an offline
  device's switch is dimmed, because a live-looking switch promises a command the
  platform cannot deliver.
- **Contrast is a gate, not a review.** `app/scripts/check-contrast.mjs` reads
  `tokens.ts` and fails `make check` when a pair that ships as text drops under
  WCAG 2.1: 4.5:1 for body and small text, 3:1 for glyphs and category accents.
  It exists because every visual review of this app found the same defect — a grey
  on a dark card, an amber "Offline" that only worked at arm's length — and neither
  a screenshot nor a reviewer settles that. The first run found **eleven** failures
  in the shipped palette, including white on system blue at 3.65:1 and Apple's
  system green at 1.99:1 as text. The fixes: the dark accent moved to `#146EF5`
  (4.59:1), the light accent to `#0062CC` (5.8:1), light-theme status colours to
  their 600–700 weights, secondary label from 60 % to 76 % alpha, and the category
  accents gained a daylight set (`categoryAccentLight`) because cyan at 1.6:1 on a
  white card is not an icon.

### Alerts are pushed, not polled

The unread count and the alert list used to be polled every 30 seconds — two requests per signed-in
app, forever. At a million apps that is ~66 000 requests a second spent asking a question the platform
already knows the answer to, so the answer is now pushed: the worker publishes
`{"t":"notification","id":…}` on `user:{owner}` when it raises an alert, the API forwards it on the
socket the app already holds, and the app bumps the count in place and refetches the list. Polling
survives as a five-minute safety net for anything a missed push left behind.

**A cache key that no screen reads is a feature that does not exist.** The first version of the live
stream wrote dashboard updates to `['home', id, 'dashboard']` while the screen fetches
`['home', id, 'dashboard', 200]` — so every pushed event landed in an entry nothing rendered, and the
dashboard never moved in the running app. The tests passed because they seeded that same wrong key.
Writes now use `setQueriesData` with the prefix (patching every page size that exists), and the test
seeds the real key so this cannot silently return.

Proven live, in this order: `0 of 6 online` → `1 of 6 online` and `Alerts · 2` → `Alerts · 3` from a
single published frame with no refresh; then a real `schedule` automation firing in the worker raised
an alert and the header went `Alerts · 3` → `Alerts · 4` after 61 s, with no client poll involved.

### The live stream has to survive a train

`useHomeStream` holds one socket per signed-in app and writes every pushed change into the query cache,
so an open screen repaints without a refetch. The version that shipped first opened the socket once and
never reopened it, which works in a demo and fails the moment a phone leaves the house.

What it does now:

- **Reconnects** with a jittered backoff — 500 ms doubling to a 30 s ceiling (`retryDelayMs`), plus up to
  250 ms of jitter so a hundred thousand phones that lost the same cell tower do not return in step.
- **Resyncs after a gap.** A reconnection means events were missed, so the dashboard and device queries
  are invalidated *when the new socket opens* — not when it is created. Being back is not being up to date.
- **Reconnects at once on resume.** A backgrounded phone is not a disconnected customer: `AppState`
  returning to `active` with a dead or closed socket reconnects immediately instead of waiting out a timer
  set twenty minutes earlier. A resume on a healthy socket does nothing.
- **Says so.** The dashboard renders **"Reconnecting — showing what was last reported"** while the stream
  is down; the status is a return value, not a swallowed detail, because a screen that silently freezes on
  yesterday's temperature is worse than one that admits it is catching up.

Verified by stopping the API under a signed-in dashboard: the notice appeared, and once the API was back
the app returned to live on its own within ~4 s, with no user action and no manual refresh.

The retry timer itself is unit-tested through `retryDelayMs` rather than by advancing a fake clock:
mounting a component under jest's fake timers leaves the next mount in the file without an effect, which
is an artefact of the environment, not something a customer meets. The drop, the resume and the status
transition are all exercised for real against the test socket.

### The silence window belongs to the product, not the platform

The 15-minute threshold was a guess that favoured honesty over accuracy: a sensor designed to report
hourly would read as "Silent" most of its life, and the platform's own presence sweep would flip it
offline every hour and back online an hour later — a device that flickers rather than one that is
understood.

So the expectation is declared data, in the product's thing model:

```json
{ "product_key": "hv_acme_sensor1", "report_interval_secs": 3600, … }
```

- **Declared**: `ThingModel.report_interval_secs`, validated 1–86 400 s (`catalog::validate_report_interval`).
  Zero is refused because it would make every device of that product permanently silent; anything over a
  day is refused because a dead device would keep looking healthy until someone noticed the reading was
  from last spring. The field is set in the portal's product editor (the thing-model JSON), so no extra
  screen was needed.
- **Copied at claim**: `devices.report_interval_secs`, in the same INSERT that copies `category` and
  `product_key` — `products` is vendor-scoped and a customer read carries no vendor, so the device
  carries its product's expectation rather than joining for it. Migration `0037` backfills what the
  catalogue already knew.
- **Used by both halves**: the app's `silenceThresholdMs` takes `max(15 min, 3 × interval)` — three
  intervals, because one missed report is normal and three is a pattern — and the worker's presence
  sweep uses the same rule in SQL, so a device is never marked offline by the platform before the app
  would call it quiet.

Verified live with a sensor declared at 3 600 s and last heard 42 minutes ago: the sweep left it
**online** past the old 90-second window, the payload carried `report_interval_secs: 3600`, and the
dashboard read **"Living Room · online"** while a plug that declares nothing read **"Silent for 4h"** on
the same screen.

### Silence is not presence, on screen too

The worker has swept for silent devices since Phase 5 — "online, but nobody has heard from it" — and the
screens did not know about it. A device that dropped off Wi-Fi an hour ago kept its online flag until
the sweep noticed, and until then the app rendered its last reading as this morning's: the tile said
"Reading · 21.5 °C", the hero said "online", and the customer's conclusion was that the platform lies.

Three states now, from one function (`entities/device/model/silence.ts`, `SILENT_AFTER_MS = 15 min`)
rather than each screen deciding what "recently" means:

| State | Pill | Second line | Hero |
|---|---|---|---|
| `live` | Online | the reading | full accent, glow, live chart |
| `stale` (online, silent) | **Silent** | **Silent for 4h** | muted value, dimmed chart, no glow |
| `offline` | Offline | Silent for 2h | as before, at 0.72 opacity |

A quiet device also stops claiming to be current in the one place it most looked like it: the hero's
number turns muted and the chart loses its accent, because a chart in full-strength accent says "this
is happening now". The device screen says it in words — *"Connected, but silent for 4h — these are the
last values it sent"* — which is the sentence someone deciding whether to walk over and look needs.

**The reason none of this could have worked before**: `last_seen_at` was serialised by `time`'s default,
a field tuple like `[2026,321,3,26,34,…]`. Every client's `new Date(...)` produced `Invalid Date`, which
the app rendered as "just now" — so a device silent for four hours looked current, and no amount of UI
work could fix it from the client side. It is an RFC 3339 string now, and `make time-serde`
(`scripts/time-serde.py`) fails the gate when any `Serialize` struct exposes an un-annotated
`OffsetDateTime` again. Verified by deleting the attribute: the gate names the file, the line and the
attribute to add.

Verified live at 390 × 844 with a device set online and last heard four hours ago: tile **Silent / Silent
for 4h**, hero muted with a dimmed chart, detail banner "Connected, but silent for 4h". The presence
sweep then flipped it to Offline on its own, which is the worker doing its job.

**The screen also says what the device promised.** "Silent for 4h" is only judgeable if the customer knows
the schedule behind it, so the device header carries one line when the product declared an interval:
*"Reports hourly · quiet after 3h without a report"*. Both numbers come from the same threshold
function, so the explanation cannot drift from the behaviour.

**The chart was drawing the battery.** The device screen charted "the first numeric point", which on a
sensor is `battery.level` — so the one screen a customer opens to see the temperature showed a battery
percentage and an empty box. `chartPoint` now prefers the reading the screen exists for
(`temperature → humidity → energy → power → brightness → battery`), the heading uses the same rule as the
control cards, and the caption carries the scale the bars were missing: **"16.6–21.5 °C · 545 readings ·
49 bars"**, with the live reading folded into the range so the label cannot contradict the number above
it. An empty window now says *"No readings in the last day — the latest is 21.5 °C."* instead of a
sentence about nothing.

Verified live on the room sensor: it went from `battery.level` / "No readings yet" to **Temperature**,
**545 readings, 49 bars**, **16.6–21.5 °C**.

Same screen, a smaller wart removed: the control headings preferred the point's `slot`, and a humidity
sensor's slot is `value` — so humidity and temperature appeared as two cards both headed **"Value"**.
`titleFor` now prefers the capability unless it only names the kind of device (`switch`, `sensor`,
`cover`, …), which gives **Humidity**, **Temperature**, **Battery**, and still **Power** for a bulb.

### A customer's own name for their own device

The server has accepted `PATCH /devices/{id} {"name": …}` since the first month; the app never offered
it, so every home was full of product names — "Acme Smart Plug (2)" where the household says "coffee
machine". The name on the device screen is now pressable and becomes the editor in place, because
renaming is the one edit a customer makes *while looking at the device* and a form that covers it hides
the context that prompted the rename.

Two rules came out of the server refusing the first attempt:

- **No control a member would be refused.** Renaming is an admin action (`403 only an admin can rename a
  device`), so the name is plain text for a member. A control that exists only to say no teaches people
  to stop touching the app.
- **A refusal is shown where the name was.** If the role changed elsewhere, the API's sentence appears
  under the header rather than the editor closing as if it had worked.

The interaction rules — draft resets on open, whitespace is a cancel, unchanged is not a request — live
in `shared/lib/useInlineEdit` and are used by the settings list too, so the two renaming surfaces cannot
drift. Verified live as an admin: typed a new name in the hero, pressed Save, and the dashboard showed it
without a reload; the demo account was returned to `member` and the device to its name afterwards.

### Removing a device, and the endpoint that never worked

A customer who gives a bulb away had no way to say so: the app offered no removal, and when the control
was added the call failed with `503 unavailable`. The route had been documented since Phase 3 and the
database was refusing it:

```
42501 new row violates row-level security policy for table "devices"
```

`home_member_scope` says "you may touch a device whose home you belong to". A release sets
`home_id = NULL`, and Postgres applies a `FOR ALL` policy's USING expression to the *new* row too — so
the row you leave behind can never be a member of the home it just left. Migrations `0038`/`0039` record
the experiment: a `WITH CHECK` that allows NULL, a split into per-command policies, `USING (… OR home_id
IS NULL)`. All of them still fail, for the same reason.

The answer is not a cleverer policy. A release *changes a row's tenancy*, which is the exact operation RLS
exists to prevent an ordinary writer from performing, so the write runs in a system context — and the
authority moves to where it can be tested: `DeviceService::unclaim` requires an admin of the home.
`backend/crates/api/tests/device_release.rs` proves both halves: **204** for an owner, and **403 with
nothing changed** for a member (before this, any member could have called it).

On the app side, "Remove this device" sits at the bottom of the device screen, quiet and destructive, and
asks in an **in-app sheet** rather than `Alert.alert` — because `react-native-web` implements that as a
no-op, which is also why "Delete room" had never worked in the web build. `shared/ui/Confirm` is a
platform-neutral, branded, testable dialog, and both destructive actions now use it.

Verified live: opened the device as an admin, pressed Remove, read the sheet, confirmed — the app returned
to `/` and the device was gone from the home. The row survived with `state = 'released'`, which is what
keeps the licence and the history honest.

### The fleet at 1 900 devices

The portal's fleet screen had never been pointed at more than a few hundred devices. The dev seed now has
1 909 (the "Plant" and "Bench house" homes plus the demo home), and the screen showed three things:

- **It asked for fifty devices a page.** Thirty-eight presses to see a vendor's own fleet, and each press was
  a full re-render of everything already loaded.
- **It said "650 devices shown · more available".** A vendor reading that cannot tell whether the rest is
  five devices or five thousand — the one number an operations screen exists to give.
- **The buttons were under the table.** At 1 900 rows that is two hundred rows of scrolling away, which on a
  phone is the same as not having them.

Now: `limit=200` (the API's maximum, so ten pages instead of thirty-eight), the total comes from
`/vendor/analytics` (`200 of 1,909 devices shown`), **Load all 1,909** walks the cursors one at a time behind
a progress label (`Loading 600 of 1,909…`) and finishes the whole fleet in **1.3 s** on this machine, and the
controls sit in a toolbar above the table with `Load more` primary and `Load all` quiet, because one is
pressed a hundred times and the other once.

Two smaller things found while measuring:

- **The nav had no active state.** Twelve destinations, all rendered as `className="muted"`, so the only way
  to know where you were was the page title. `NavLink` now marks the current one — weight as well as colour,
  because a colour alone is not a signal for everyone — and sets `aria-current`.
- **`content-visibility: auto` on table rows**, with `contain-intrinsic-size` so the scrollbar stays honest.
  Measured honestly: at 1 909 rows it moved p95 frame time from **17.2 ms to 16.9 ms** and the worst frame
  from 19.4 to 17.7 while scrolling — real, and small, because Chrome was already coping. It is there for the
  fleet sizes where it is not small, and it costs nothing to have.

### "Load more" is a cursor, not a bigger number

The footer used to raise `device_limit` (`limit * 4`) and ask again. The server clamps a page at **500**, so
on a home of 1 200 devices the second press requested 2 000, was given the same first 500, and the footer
kept offering devices that could never arrive — the last 700 were unreachable in the app while the button
said "Load 1 000 more". Measured, not imagined: the 420-device bench home fit under the cap, which is why it
never showed.

"More" is now a list of cursors (`useDashboardTail`). Each page is fetched with `device_limit=500` and the
cursor of the last loaded page, added under the dashboard's own key prefix — so the live stream patches
tail pages with the same `setQueriesData`, and a reconnect invalidates them all. The button says what it
will do, not what is left in the home: **Load 500 more · showing 200 of 1200**.

At 1 200 devices: press → 700, press → 1 200 and the footer disappears. 80 tiles mounted at 1 200.

### The rights are in the product, not in the policy

`GET /v1/me/export` and `DELETE /v1/me` have existed since Phase 6, with receipts, retention windows and
a purge worker behind them. The app had neither screen — so a customer was legally owed a copy of their
data and a way to delete it, and the only route to either was to ask a person.

`/privacy` (Account section of Settings) now does both:

- **Download a copy** fetches the bundle, names the file the way the server does
  (`haven-export-YYYYMMDD.json`), saves it — an anchor download on web, a file plus share sheet on a
  phone — and says what was saved: *"Saved haven-export-20261117.json · 2 homes · 1017 readings · 3
  notifications"*. Naming the counts is the difference between a file and a file you can trust.
- **Delete my account** asks in the app's own sheet, then shows the receipt **before** signing out:
  *"0 homes and 0 devices deleted · 2 sessions signed out · a receipt kept for 90 days"*. The first
  version signed out immediately, which unmounted the screen through the session gate and dropped the
  person at a login form with no confirmation that anything had happened — found by driving it with a
  throwaway account.

Both endpoints act on the caller and nobody else, so there is no id to guess; the screen adds no new
authority, only a way to use it. Verified live: exported the demo account's data from the phone-sized
web build, then registered a throwaway account, deleted it from the screen, read the receipt and was
signed out.

### A chart that stops where the readings stop

The hero card led with a 24-hour line and the live reading. Three versions of that chart were wrong before the
fourth was honest, and each looked like a rendering bug because each *was* one:

1. **Appending the live value** put two points at the same x — the last bucket and "now" are the same hour — and
   drew a vertical cliff at the right edge.
2. **Overwriting the last bucket** with the live value was worse: a bucket from 03:36 was painted as if it held a
   21:30 reading, and the footer printed a maximum the line never reached.
3. **An index-based x axis** hid the real problem: a twelve-hour gap between the last reading and now occupied
   fourteen pixels, so the card said *"10 PM–now"* above a line that visibly stopped at 11 AM.

What it does now:

- the line is the recorded history, **placed at each bucket's own timestamp** — a gap takes the space it took;
- the live reading is a **separate marker past a gutter**, joined by a **dotted** segment (solid would claim those
  hours were measured; a floating dot with no line reads as a mistake);
- the axis names the first bucket with its day (`Yest 10 PM`) and ends at `now`;
- the footer prints the range **including the live value** — otherwise a 21.5 sat above a printed maximum of 20.7 —
  and, when there is a gap, says what it is: *"11 h without a reading"*.

The gap threshold lives in one function (`gapHours`), because the marker and the caption must not be able to
disagree, and "one bucket behind now" is a bucket being filled rather than missing data.

Vision reviews traced the card **6.5 → 7 → 7.5 → 9 / 10** across those four versions; the last one still called
the *screen* 7.5–8, so the card is no longer the weak part.

### The header answers a question the clock could not

The top-right pill was a clock with a lightning bolt beside it, in warning orange: a redundant time (the OS
draws one in the status bar) under a glyph that means *power*, coloured like a fault, on a healthy house.

It is now the presence pill — `5 offline` on an amber dot, `1 of 200 online` in green when the house is
whole, `no devices` on an empty home — and it is the only place the number appears. The line under the title
took over the job the pill left behind: `6 devices · 3 rooms`, or `1,200 devices · showing 200` when the grid
is a page of a bigger home. The banner below says *why* (`Some devices are not reporting — showing what they
last sent`) and deliberately no longer repeats the count, because a number in two vocabularies reads as two
problems.

The rail lost its `Rooms` card for the same reason: the subtitle already says how many rooms there are, and a
rail that repeats the line above it is the cheapest way to look busy.

The hero card became a door. It was a large card that navigated nowhere and offered no chevron, so the most
prominent thing on the screen was the one thing that did not respond to a tap; it now opens the device it
describes and says so. When its reading is stale the chart fades to 58 % of the category accent and the
number to 74 % white — enough contrast to read a 44-point figure, not enough to mistake it for live.

### One fact per tile

A tile used to carry a pill (`Offline`), a line under the name (`Silent for 1h`) and, for a sensor, a value
plus a caption naming its capability. Three sentences for one device, and on a home where something was wrong
every tile repeated the same two words in two vocabularies.

Now a tile says one thing about state: `Offline · 1h ago`, `Silent for 4h`, `Updated just now`, `On`, `Off`.
The coloured dot keeps the colour that the pill used to carry, the words carry the age, and the value appears
once — as the headline for a sensor, as the switch for a bulb. The same rule applies at every level:
`rows.ts` counts a room's online devices with `freshness()`, not with the server's `online` flag, because a
room header saying `1/2 online` above a tile with an amber dot is two answers to one question.

**A tile had to be told, twice, how wide it is.** The horizontal row (icon, name, control) is right for a
single-device section and wrong for a half-width tile: at 390 points a two-column tile has about 140 points
of content, so icon + name + switch left forty for the name and `Floor Lamp` became `F…`. Grid tiles stack
(icon and control on top, name and state underneath); the wide row keeps them side by side.

**`Glass` derives its alphas from an opaque accent.** Passing `withAlpha(accent, 0.45)` to mean "a quieter
stale tile" produced `rgba(34, 211, 238, 0.45)` as the card's *background* — brighter than a live tile,
because the internal `withAlpha` could not parse the string it was given and fell back to the value itself.
Staleness is now expressed by dropping the halo and stepping the card's opacity back, with the accent left
opaque for the component to fade in its own way.

### A screen that asks the session a question the session has not answered

`/scenes` opened with **no scenes and no devices** on a home with both — and offered a form whose device
picker said "Nothing to choose from", whose point picker said "Pick a device first", and whose value field
already contained `true`. Scored 3/10.

The cause was one line, and it was the same line in `/settings`: both read `useSession(s => s.homeId)`, and
that is empty until the *dashboard* picks a home. Any screen reached first therefore had no home. `useHomes`
has the answer already, so `useCurrentHome()` returns the stored home, or the one the app would open, and
writes that choice down — the screens that ran after the dashboard were fine by accident, which is exactly
why nobody noticed.

The editor was rebuilt around progressive disclosure, which is what the old one was missing:

- **Name → choose device → what should happen → set to.** Nothing appears before the step that makes it
  meaningful; "What should happen" does not exist until a device is chosen, and the value control does not
  exist until a point is.
- **Points are named in words with their units** — `Level (%)`, `Kelvin (K)`, `Power` — not `brightness.level`.
  The wire name is a vendor's identifier.
- **The value control matches the type**: On/Off chips for a switch, a numeric field with the unit for a level,
  text otherwise. The old single box said "Value (true / false / number)" and asked the customer to know the
  wire format of their own bulb.
- **Read-only points are not offered at all.** A scene cannot set a sensor; the editor says so ("This device
  has nothing a scene can change") rather than listing points that will fail.
- **`Add another action` appears only when the current action is complete** — a control that offers more work
  before the first piece exists invites a mess.

**A disabled button must look disabled.** `Button` set `opacity: 0.5` on a filled accent, which on a dark
screen is still a blue button; iOS greys the fill *and* the label. Every disabled button in the app now does.
The greyed `Create scene` also says what is missing ("Name the scene to finish."), because a grey button with
no reason is a puzzle.

Scenes **3 → 8/10**.

### The switcher has to say which one you are in

`/homes` scored **4/10** and the reason was one missing line: no row said "Current". The check and the label
were in the code — the session's `homeId` is empty until the dashboard picks a home, so on a fresh sign-in
every row failed `home.id === current` and the screen that exists to tell you where you are told you nothing.
It falls back to the home the app would open (the first one) until a choice is stored.

Three smaller corrections in the same pass:

- **`2 people · admin` read as three counts.** "admin" is a fact about *you*, not about the home, and it sat
  in a line of numbers. The role moved to a chip beside the name — and the chip is drawn only when the
  account holds *different* roles in different homes, because a word repeated on every row is decoration.
- **Every home wore the same house glyph.** The rows now use the home's initial in an accent circle, the way
  the dashboard already marks the person. Three identical houses is a list you read; W / B / P is a list you
  scan.
- **`1200 devices`** became `1,200 devices` — `toLocaleString`, not string concatenation.

Homes **4 → 9/10**.

### A count is a fact about the home, not about the page you fetched

Settings said `Devices 0` on a home with six devices. The row printed `dashboard.devices.length` — one page
of a cursor, and on this screen usually none of it — and then printed `0` for the half-second before the
answer arrived, which is the same lie with a shorter lifetime. It now reads the home's `devices_total`, and
an em dash while that is in flight.

Two other rows were lying in the same way:

- **`Add a room` created a room called "New room"** the moment it was touched — no name, no confirmation, and
  a chevron on a row that navigated nowhere. It asks for the name first, and `Row` learned `chevron={false}`
  for rows that expand in place instead of going somewhere.
- **`Back to home` sat under ABOUT.** Navigation is not information about the app; it is the header's job, so
  the screen uses `ScreenHeader` like every other pushed screen and ABOUT holds the product name and version.

**A member was offered three controls that answered 403.** `Delete`, the room editor and `Add a room` were
drawn for anyone, and the refusals were silent — the demo account *was* a member of its own seeded home,
which is how a screen looks broken when the server is behaving correctly. The role ships with the homes list,
so the controls are drawn only when `canManage(role)` allows them, with the reason in the section footnote.

### The two reasons a first pairing fails

The Wi-Fi form — the step a customer must complete for any device to exist at all — asked for `Network name`
and `Password` as placeholder-only fields and nothing else. Both reasons a first pairing fails were unspoken:

- **These devices are 2.4 GHz only.** A house with `Home` and `Home_5G` looks like one network until the device
  never appears, and the customer has no way to know which name was the wrong one. The form says it before the
  mistake: *"Most smart devices join 2.4 GHz only. If you can see two names, pick the one without “5G”."*
- **A Wi-Fi password is typed blind on a phone keyboard**, and a typo here looks exactly like a broken device.
  There is a `SHOW` / `HIDE` control, as on the sign-in screen.

Both fields are labelled, not placeholder-only, and the password field carries `autoComplete="off"` so a browser
does not offer to fill *the platform's* password into *the router's* field.

Verified through the pairing suite (real render, mock radio): the labels are found the way a person finds them,
the 2.4 GHz line is present, and the toggle flips `secureTextEntry`. What is *not* verified is the step after
this one — handing credentials to a real device over BLE — because the browser has no radio; that remains a
hardware-day item.

### The first screen a customer ever sees

The sign-in screen scored **5/10** as a white-label first impression, and the reasons were all "the form does
not help the person typing into it":

- **A placeholder is not a label.** `Email` and `Password` vanished on the first keystroke, leaving two unnamed
  boxes. Both fields have a persistent label now.
- **No way to see what you typed.** Typing a password blind on a phone keyboard is the most common reason a
  sign-in fails, and the customer blames the app. There is a `SHOW` / `HIDE` control inside the field.
- **`autoComplete="password"`** made both modes look like a *new-password* field, so a password manager offered
  to generate a password instead of filling the saved one. Sign-in now says `current-password` (and
  `new-password` in the create-account mode), with `textContentType` to match on iOS.
- **"No account yet? Create one"** became `New to Haven? Create an account` — in the operator's product name,
  since this screen wears their brand.

Verified live at 390×844: labels and `SHOW` render, the toggle switches the field from password to text (the
DOM input type changes from `password` to `text`), the enabled `Sign in` is a filled accent button rather than
the grey disabled style, and the flow still signs in — the dashboard loads as `demo@haven.local`.

Sign-in **5 → 7/10**. What it still lacks is a password reset (there is no such endpoint yet) and any legal or
support link, which is a product decision rather than a coat of paint.

### The automations editor asked for a wire format

`/automations` scored **7/10** and the empty state was the smaller half of it. The title said **`Automatic`**
while the tab, the route and the product all say *Automations*; the empty card explained what a rule is and
then stopped, leaving the only way to start as a ghost `New rule` link two lines above it; and the editor asked
for **`Point`** with the choices `switch.power`, `brightness.level`, `color_temp.kelvin` and a text box labelled
`Set to (true / false / number)`.

Same disease as the scenes editor one iteration earlier, and the cure is now shared: `ValueField`,
`readablePoint` and `defaultPointValue` moved into `shared/ui`, so both editors label a point `Power` /
`Level (%)` and draw a switch for a switch, a numeric field with its unit for a level, and text for text. Two
copies of that control is how two slightly different controls happen — the reason `ChipPicker` exists at all.

Three more from the same review:

- **The empty state keeps its own action** (`Create your first rule`), and it is hidden while the form is open —
  a card under the editor that says "no automations" is the screen contradicting itself in two places.
- **`When` was a ghost text button beside a full-width primary one** — two controls from two design systems for
  one either/or question. It is a chip row now, like every other choice on the screen.
- **A bare `5`** under a schedule is a number with no unit; it says `EVERY (MINUTES)`.

Also fixed in `ChipPicker` itself: an unselected chip printed its label in muted grey on a surface that is
nearly the same grey, so half the options were there and unreadable. Selection is carried by the fill.

Automations **7 → 8/10**.

### The device screen was a debug view with a chart

`/device/:id` scored **6/10** and every deduction was the same mistake in a different place: it printed the
platform's own vocabulary.

- **`battery.level · %`** under a row already titled *Battery*, with the unit beside the value anyway. The wire
  name is a vendor's identifier; the customer has never seen it.
- **`quiet after 3h without a report`** described the *threshold the platform uses*, not a fact about their
  device. It reads `Reports hourly · last reported 4h ago` now: what it promised, and when it last kept it.
- **`284 readings · 26 bars`** counted samples stored and columns drawn — plumbing, on the screen where the
  customer asked what the temperature had been doing. The caption is the range and the window.
- **Battery came first**, because `device.points` arrives sorted by code and `battery.level` sorts before
  `temperature.value`. Readings are ordered the way a person reads them now: temperature, humidity, then
  battery, because battery is about the device rather than the room.
- **The chart did not admit it had stopped.** A device offline for four hours still drew a confident day of
  bars; the caption now ends `· as of 4h ago`.

Device detail **6 → 8/10**.

### A feed needs a day, not a count of hours

`/alerts` had three alerts and no way back — it is pushed from the bell, and the only exit was the browser's
own gesture. It also said `10 h ago` under no heading at all, so eight rows from four days read as one
undifferentiated blur, and a read row and an unread row differed by a three-pixel dot.

Now: a header with the back chevron `ScreenHeader` already knew how to draw, alerts grouped under `TODAY` /
`YESTERDAY` / a date, and **the row inside a group shows the time of day** — `2:21 PM`. A heading that says
which day plus a row that says "10h ago" is one fact in two scales; once the group answers *when*, the row
answers *what time*. Read rows step back to 72 % instead of relying on the dot alone, and the card carries an
accessible name — `Lamp on, unread, opens Room Sensor` — because neither the dot nor the dimming is audible.

The third alert read `Lease check` / `the doorbell rang` with a warning triangle: a seeded row whose title and
body came from different stories. It reads `Doorbell rang` / `Someone pressed the doorbell in the hallway.`
now, with the security glyph. **Demo data is copy**: a screenshot of the product is a screenshot of whatever
the seed put there, and a mismatched title is indistinguishable from a bug.

The per-row action line names the row's primary action — `Open Room Sensor` where there is a device,
`Mark as read` where there is not — and tapping anywhere does both. One line, one promise.

### The first screen is not allowed to be a dead end

`/add-device` in a browser rendered a heading — "Add a device" — and, under it, `No radio here / This build
cannot scan for Bluetooth devices. Pairing needs the phone app.` Then nothing. The first screen a new
customer sees was a developer's sentence about a limitation they could not act on, on an empty page, with no
button, no link and no way back. Scored **4/10**.

It is a handoff now: a card with a phone glyph, `Pair from your phone`, one line of why (a browser cannot
scan for Bluetooth; everything else works here), three numbered steps ending in what will happen
("it will appear within a few seconds"), a `Copy the phone link` button that copies `haven://add-device`,
and `Back to your home`. Scored **8/10**.

Two rules came out of it that apply to every screen still to be reviewed:

- **A limitation the customer cannot fix is not the message.** Say what to do instead, in their words.
- **Every state needs an exit.** The old screen's only affordance was the browser's back button; a state
  that cannot be left is a bug even when it is honest.

The content scrolls (`ScrollView`) rather than clipping: four blocks of text on a 667-point phone with the
text size turned up is exactly where a fixed layout loses its last button.

### One button, and it asks

Three creates lived in three places: the centre button went straight to a device scan, `New room` was a chip
among the filters, and `Add a scene` was a dashed card in the strip. A customer looking for "add something"
had to know *which* something before they could find *where* to say it.

The centre button now opens a short sheet — `Add to Workshop`: a device, a room, a scene, each with one line
saying what it is for. The rows are sentences rather than a grid of icons because the labels carry the
distinction ("One tap that sets several devices"), and a sheet you can read is faster than a grid you have to
decode. Picking "Add a room" reveals the name field inline, in the list it will join, with its own Cancel.

The empty scene strip is gone with it: an empty state that duplicates a menu item is a border around nothing.

**The sheet asks the server's question before it asks the customer's.** `POST /homes/{id}/rooms` answers
`403 only an admin can add a room` to a member — and the demo account *is* a member of the seeded home, so
pressing "Add a room" did nothing at all, silently. The role arrives with the homes list, so the option is now
drawn only when `canManage(role)`; and if the API refuses anyway, the form prints the reason under the field
instead of failing quietly.

### Three jobs were stacked as three identical pills

Under the hero card sat `Alerts · 2` beside `New room` — a status and a create action, drawn the same
way — and below them a search field, a filter row, and a dashed `+ Add a scene`. Five controls of four
different kinds in one column, all competing with the devices they were supposed to help you find.

- **The alerts count is a bell in the header.** A notification is a status, and it belongs where the
  presence pill is; as a pill next to a create action it read as a filter. One orange thing in that corner,
  not two: the badge is enough.
- **`+ Room` sits at the end of the filter row**, which is the row that filters the rooms it makes. A create
  action in a row of filters is still a compromise, but a smaller one than floating above the search box.
- **`+ Add a scene` stayed with the scenes** — it is the empty state of that strip, not a control on the
  device list.
- The search field kept the full width and the filters stayed directly under it, because that pair is the
  one grouping here that never confused anybody.

Measured: header 6 → 9/10, control area 7 → 8/10 across the two reviews.

### The heartbeat is not the reading

A sensor with a healthy socket and a twelve-hour-old temperature satisfied every check: `online` was true,
`last_seen_at` was a second old, and the tile showed a green dot beside `Updated 12h ago`, counted as online
in the room header. `freshness()` now compares the **oldest non-writable point** against the same silence
threshold, so `readingAgeMs()` decides one answer for the dot, the words, the room count and the header pill.
Writable points are exempt: a bulb that has been off for three days is off, not silent.

### Measure at the viewport the customer has

Half a day of "the floating tab bar covers the hero card" turned out to be a lie told by the screenshot rig:
`Emulation.setDeviceMetricsOverride({ width: 390, height: 844, deviceScaleFactor: 2 })` gives a **413 px**
CSS viewport, because the height is in device pixels. At a real 844 px the hero card ends at 510 and the tab
bar starts at 760 — 250 px of clearance, exactly as designed. Any layout claim from a screenshot is worth
nothing until `innerHeight` agrees with the device you are pretending to be.

### A reading has its own clock

`last_seen_at` answers *is the device reachable*. It does not answer *how old is this number*, and for two
iterations the dashboard pretended otherwise: the hero sensor said `Living Room · online` above a chart that
stopped eleven hours earlier, with a 44-point `21.5` that looked like right now.

The reading's timestamp was in the database from the first migration. The API never selected it. With
`updated_at` on every point the card can be honest in four places at once:

| The claim | Before | Now |
|---|---|---|
| the connection | `online` | `11 h since the last reading` when the series has a gap |
| the number | full-strength, 44 pt | dimmed to 72 % with a `last reading` tag |
| the chart | marker at the right edge | marker **at the reading's own moment**, empty space to `now` |
| the gap | nothing | `11 h without a reading`, on a muted dotted connector |

The x-axis still ends at *now* — that is what makes a gap look like a gap instead of pushing the last
reading against the right edge and calling it current. Placing the marker at its own timestamp inside that
window is the whole fix: a chart cannot draw a reading at a time no reading exists for.

Two smaller corrections came out of the same review: the stat rail counts **lights that are on and still
reporting** (`lightsOnLive`), because a bulb whose last word was "on" twenty minutes before it went quiet is
not a light that is on now; and one banner above everything says `5 devices are not reporting — showing what
they last sent`, so the rail's `0 on` and the tiles' `Offline` read as facts about the network rather than
about the bulbs.

### Colour is a token, or it is a bug

`src/shared/config/tokens.ts` is the only file in the app allowed to contain a colour value, and
`make tokens` (`app/scripts/check-tokens.mjs`) fails the gate on any hex or `rgba()`/`hsl()` literal
elsewhere in `src`. Tests and comments are exempt — a comment may discuss a colour without declaring
one.

The gate exists because the operator owns the brand at runtime. A screen that hardcodes
`rgba(255,255,255,0.16)` for a dashed border overrides them, and the light appearance is the one that
finds out: that border was invisible on a light canvas, in the scene strip and in the alerts list,
until this gate was added and the values moved into the palette. `withAlpha()` lives in the same file
for the same reason — one implementation of "hex plus alpha", not two that drift.

Two consequences worth stating:

- **Chrome geometry is a token too** (`tokens.layout.tabBarClearance`). The floating tab bar is
  absolute over the content, so every scrollable screen has to end above it; four screens had guessed
  their own padding and one had none, which is why the last device row sat under the bar.
- **Light is verified, not assumed.** The operator can choose the appearance, so it was rendered at
  390 × 844 and reviewed: opaque chrome with a hairline and a shadow, muted text at 5.88:1 on the
  canvas, and the last device row clearing the bar.

### More than one home, and more than one page

Tapping the home's name opens `/homes`: every home the account can open, with its device and member
counts from `GET /homes`, the current one ticked, and a way to create another. The app used to store
one home id and assume it was enough — wrong for a customer with a flat and a cottage, and wrong for
anyone who needs to look at a large account without logging in as its owner.

The switcher is also how the paged dashboard is verified: the demo home has six devices and never
shows a cursor, so `scripts/seed-big-home.sql` adds a **420-device** home. Measured on it:

- the first page loads 200 devices and the header reads `171 of 420 online · showing 200` — the
  count is the home's, not the page's;
- the footer reads **Load 220 more · showing 200 of 420** and loads the rest in one press;
- with all 420 loaded, **48 device tiles are mounted** at a time, because the list is windowed.

### The brand is data, not a constant

`tokens.ts` used to be the whole brand, which meant a white-label partner needed a fork. It is now the
**default** brand. `GET /v1/theme` returns the operator's (`product_name`, `logo_text`, `accent`,
`accent_secondary`, `mode`, and a computed `accent_on`), and `PlatformThemeProvider` turns that into
the palette every screen already reads — no screen changed to become white-label.

- **Only three things move**: the accent, the text colour that sits on it, and the halo derived from
  it. Backgrounds, text tiers and separators stay the system's: a customer buys a product that still
  behaves like a phone, not a colour picker that can make a screen unreadable.
- **The server decides readability.** `accent_on` arrives computed by luminance; the app never guesses
  and the console's preview applies the same rule, so what the operator sees is what ships.
- **It survives a cold start offline.** The last good answer is cached in `expo-secure-store` and used
  before the first paint; a failed fetch, a 500 or a malformed payload all fall back to the built-in
  palette rather than to a blank screen.
- **It is cached for an hour in the client and a minute on the wire.** A brand changes when an
  operator decides to, not between two launches.

### Demo content

`scripts/seed-demo.sql` fills the demo account's home with the products the vendor
catalogue already carries (bulbs, a plug, a climate sensor), three rooms, and a day of
temperature telemetry so the hero has a shape to draw. It is idempotent and it upserts
the point *values*, because the demo's values are the point of the demo.

It is static data: the presence sweep marks anything that stops reporting offline after
90 seconds, which is correct for a device and useless for a screenshot. Re-run the seed
for a fresh frame, or attach a real/simulated device for a live one.

### Expo MCP

`expo-mcp` is a dev dependency and `npm run start:mcp` starts the dev server with
`EXPO_UNSTABLE_MCP_SERVER=1`, which is what exposes the local capabilities
(screenshot, tap by testID, router sitemap) to an MCP client.

Two honest limits on this machine: the **remote** server (`https://mcp.expo.dev/mcp`,
OAuth) needs an MCP client to register it — Hermes has no Expo entry in its
connector catalog — and the **local** capabilities need an iOS Simulator, which
needs a full Xcode install. This machine has only CommandLineTools, so screens are
verified through the web build instead. On a Mac with Xcode the same code path
gives real screenshots and taps.

### A module that cannot load must not take the app with it

`react-native-ble-plx` throws while being **constructed** when the native module is
absent — which is every browser, and any build without the native dependency. A
module-scope `new BlePlxRadio()` therefore killed the whole app on web: expo-router
imports every route eagerly, so opening the login screen imported the radar route,
which imported the radio, which threw inside `<ContextNavigator>` and blanked the
tree.

Two rules came out of it, and both are now enforced:

1. **Nothing above a screen constructs a platform module at import time.**
   `getRadio()` builds one on first use, `require`s the native implementation lazily,
   and falls back to `NullRadio` when that throws. A browser now shows "No radio
   here — pairing needs the phone app".
2. **A render error is a bad screen, not a dead app.** `app-shell/ErrorBoundary.tsx`
   wraps the router tree, shows what failed, and offers a retry.

The `Unable to run simctl` line in `make app` is Expo detecting no iOS Simulator
(CommandLineTools, not full Xcode). It is informational and unrelated.

