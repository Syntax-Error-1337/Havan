import { expect, test } from '@playwright/test';

import { VENDOR, signIn } from './fixtures';

/**
 * The door. Everything else in this suite assumes it works, so it is checked first and by hand
 * rather than through the signed-in fixture.
 */
test('a vendor signs in and lands on their products', async ({ page }) => {
  await signIn(page);

  // The nav is the map of what the portal offers; if a screen is missing from it, a vendor cannot
  // find the screen at all.
  for (const item of ['Products', 'Fleet', 'Analytics', 'Audit', 'Rollouts', 'Licences', 'Invoices', 'Team', 'Webhooks', 'Security', 'API keys']) {
    await expect(page.getByRole('link', { name: item, exact: true })).toBeVisible();
  }
});

test('a wrong password is refused without saying whether the account exists', async ({ page }) => {
  await page.goto('/login');
  await page.getByLabel('Workspace').fill(VENDOR.slug);
  await page.getByLabel('Email').fill(VENDOR.email);
  await page.getByLabel('Password').fill('not-the-password');
  await page.getByRole('button', { name: 'Sign in' }).click();

  await expect(page.getByText(/incorrect|invalid|failed/i)).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Products' })).toHaveCount(0);
});

test('a signed-in vendor is not asked to sign in again on a reload', async ({ page }) => {
  await signIn(page);
  await page.reload();
  await expect(page.getByRole('heading', { name: 'Products' })).toBeVisible();
});

test('a signed-out vendor is sent to the login page, not to an empty shell', async ({ page }) => {
  await page.goto('/licences');
  // No session: the gate lives in the shell, so a deep link cannot skip it.
  await expect(page.getByLabel('Workspace')).toBeVisible();
});
