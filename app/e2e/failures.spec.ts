import { expect, test, type Page } from '@playwright/test';

/**
 * The states nobody screenshots: what the customer sees when the network fails.
 *
 * Three screens had *no* failure state at all. `/automations` used `{ data: rules, isLoading }` with no `isError`,
 * so a failed fetch left `rules` undefined and the screen said **"Nothing runs on its own yet"** — inviting the
 * customer to create a rule on a hub it could not reach. The device detail said "Device unavailable" and offered
 * nothing to do about it. `/homes` printed the server's reason and no way to ask again.
 *
 * These tests cut the connection at the browser rather than mocking the app, so what fails is the real request.
 */

const DEMO = {
  email: process.env.E2E_DEMO_EMAIL ?? 'demo@haven.local',
  password: process.env.E2E_DEMO_PASSWORD ?? 'haven12345',
};

async function signIn(page: Page) {
  await page.goto('/');
  await page.getByLabel('Email', { exact: true }).fill(DEMO.email);
  await page.getByLabel('Password', { exact: true }).fill(DEMO.password);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(page.getByText('DASHBOARD')).toBeVisible();
}

test('a failed automation fetch is not "you have no rules"', async ({ page }) => {
  await signIn(page);

  // The API call, not the route: `**/automations` also matched the *page* URL, and aborting that is a
  // navigation failure rather than the screen's own failure state.
  let broken = true;
  await page.route('**/v1/homes/*/automations*', (route) => {
    if (broken) return route.abort('failed');
    return route.continue();
  });

  await page.goto('/automations');
  await expect(page.getByText('We could not load your automations')).toBeVisible();
  // The sentence that matters: nothing has changed.
  await expect(page.getByText(/rules on your hub are still running/)).toBeVisible();
  // And the empty state must not be there to invite a rule that cannot be saved.
  await expect(page.getByText('Nothing runs on its own yet')).toHaveCount(0);

  broken = false;
  await page.getByRole('button', { name: 'Try again' }).click();
  await expect(page.getByText('We could not load your automations')).toHaveCount(0);
});

test('a device that cannot be loaded offers a way to try again', async ({ page }) => {
  await signIn(page);

  // The device list loads, then the device itself fails: the customer reached a real row and the screen broke.
  let broken = false;
  await page.route('**/v1/devices/**', (route) => {
    // The list request is `/v1/devices?home_id=…`; only a single device is `/v1/devices/<uuid>`.
    // Any single-device path: the demo ids are seeds (`SEED-SENSOR-1`), not uuids.
    if (broken && /^\/v1\/devices\/[^/]+$/.test(new URL(route.request().url()).pathname)) {
      return route.abort('failed');
    }
    return route.continue();
  });

  // The tile, by id: "Room Sensor" also names the hero card that replaces it once the device opens.
  await page.getByTestId('device-tile-SEED-SENSOR-1').click();
  await expect(page.getByTestId('hero-card')).toBeVisible();

  // A cold navigation to the device, which is what a push notification does — and what makes this a real
  // request rather than a cache hit.
  broken = true;
  await page.goto('/device/01a1fb00-0000-7000-8000-000000000002');
  await expect(page.getByText('Device unavailable')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible();
});
