import { defineConfig, devices } from '@playwright/test';

/**
 * The vendor portal's own tests, run against a real API and a real database.
 *
 * `npm run test:e2e` expects the stack that `make up` starts; the dev server is reused when it is
 * already running so a session doing manual checks does not fight the suite for port 5173.
 *
 * Two projects on purpose: a desktop pass, and a 420 px pass that catches a table which overflows
 * the page instead of scrolling inside its frame — the failure that only appears on a laptop with
 * the window snapped to one side.
 */
export default defineConfig({
  testDir: './e2e',
  // One worker: the specs share the seeded vendor account, and a seat is a limited resource in a
  // test that invites a colleague.
  workers: 1,
  fullyParallel: false,
  timeout: 30_000,
  expect: { timeout: 10_000 },
  reporter: [['list']],
  use: {
    baseURL: process.env.PORTAL_URL ?? 'http://127.0.0.1:5173',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'desktop', use: { ...devices['Desktop Chrome'], viewport: { width: 1280, height: 900 } } },
    // Chromium, not WebKit: this project exists to prove the layout survives a phone viewport, and
    // downloading a second engine for a width check costs CI minutes to prove nothing extra.
    { name: 'phone', use: { ...devices['Pixel 7'] } },
  ],
  webServer: {
    command: 'npm run dev -- --port 5173 --strictPort',
    url: 'http://127.0.0.1:5173',
    reuseExistingServer: true,
    timeout: 60_000,
  },
});
