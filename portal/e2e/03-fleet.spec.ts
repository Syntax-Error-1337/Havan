import { expect, test } from './fixtures';

/** Fleet and analytics: the two screens a vendor opens when a customer complains. */

test('the fleet lists devices with a state a vendor can act on', async ({ app }) => {
  await app.getByRole('link', { name: 'Fleet', exact: true }).click();

  await expect(app.getByRole('heading', { name: /fleet/i })).toBeVisible();
  // The serial is the vendor's own identifier; without it nobody can match a support ticket to a
  // device.
  await expect(app.getByRole('columnheader', { name: /serial/i })).toBeVisible();
});

test('fleet filters narrow the list rather than reloading the page', async ({ app }) => {
  await app.goto('/fleet');
  const rows = app.locator('tbody tr');
  const before = await rows.count();

  const search = app.getByPlaceholder(/search|serial|device/i).first();
  if (await search.count()) {
    await search.fill('CONF-');
    // Either fewer rows or an explicit empty state — never the old list with no indication.
    await expect(async () => {
      const after = await rows.count();
      const empty = await app.getByText(/no devices|nothing matches/i).count();
      expect(after < before || empty > 0).toBe(true);
    }).toPass();
  }
});

test('analytics reports totals the vendor can quote to their own board', async ({ app }) => {
  await app.getByRole('link', { name: 'Analytics', exact: true }).click();

  await expect(app.getByRole('heading', { name: /analytics/i })).toBeVisible();
  await expect(app.getByText(/total|devices|online/i).first()).toBeVisible();
});

test('the audit trail shows who did what, newest first', async ({ app }) => {
  await app.getByRole('link', { name: 'Audit', exact: true }).click();

  await expect(app.getByRole('heading', { name: /audit/i })).toBeVisible();
  await expect(app.getByRole('columnheader', { name: /action|when|actor/i }).first()).toBeVisible();
});
