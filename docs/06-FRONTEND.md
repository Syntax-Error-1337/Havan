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

## Portal (React)

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

