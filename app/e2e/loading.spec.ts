import { expect, test, type Page } from '@playwright/test';

/**
 * Loading is a state, and it has a shape.
 *
 * Alerts, automations and homes each said **"Loading…"** — one grey word in the middle of an otherwise empty
 * screen. On a slow connection that is a second of a screen that looks broken, and then the layout jumps when
 * the rows arrive. These tests hold the API response open and assert the screen is drawing the list it is
 * waiting for, not a word.
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

test('a list that is loading draws itself, not the word "Loading"', async ({ page }) => {
  await signIn(page);

  // Hold the alerts response for a moment: the skeleton is only visible while something is pending.
  await page.route('**/v1/notifications**', async (route) => {
    await new Promise((resolve) => setTimeout(resolve, 2_500));
    await route.continue();
  });

  await page.goto('/alerts');
  await expect(page.getByTestId('list-skeleton')).toBeVisible();
  await expect(page.getByText('Loading…')).toHaveCount(0);
  // And it does not also make a claim it cannot check: the feed said "Nothing new" above three skeletons.
  await expect(page.getByText('Nothing new')).toHaveCount(0);

  // And the real list replaces it rather than stacking under it.
  await expect(page.getByTestId('list-skeleton')).toHaveCount(0, { timeout: 15_000 });
});
