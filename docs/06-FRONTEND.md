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
