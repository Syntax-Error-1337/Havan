import { defineConfig, devices } from '@playwright/test';

/**
 * The customer app's own end-to-end tests.
 *
 * The portal has had a suite for months and it earned its keep last week by finding a sideways scrollbar
 * that no component test and no screenshot review could see. The app — the product the customer actually
 * holds — was verified by unit tests and by hand. This closes that gap for the journeys that must work
 * before anything else does: sign in, see the home, open a device, look at a rule, change an appearance.
 *
 * It runs against the Expo web build (`make dev-mobile`), which is not a native build and is not claimed
 * to be: what it proves is wiring, routing, and that each screen renders its own data. A native pass
 * still needs a phone.
 */
export default defineConfig({
  testDir: './e2e',
  // One worker: the specs share the demo account, and a suite that signs out while another signs in is
  // testing its own races.
  workers: 1,
  fullyParallel: false,
  timeout: 45_000,
  expect: { timeout: 15_000 },
  reporter: [['list']],
  use: {
    baseURL: process.env.APP_URL ?? 'http://127.0.0.1:8082',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    // 390 x 844 is the phone this app is designed for; the layout is a phone layout, so a desktop
    // project would prove nothing a screenshot does not already show.
    { name: 'phone', use: { ...devices['Pixel 7'], viewport: { width: 390, height: 844 } } },
  ],
});
