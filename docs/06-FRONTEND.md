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

