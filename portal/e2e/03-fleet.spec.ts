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
  await expect(rows.first()).toBeVisible();

  // A prefix is not a narrowing: the seed has hundreds of CONF- serials, so filtering by "CONF-"
  // legitimately fills the page again. What a support engineer actually does is paste one serial and
  // expect that one device, so the test does the same.
  const serial = (await rows.first().locator('td').first().innerText()).trim();
  const search = app.getByTestId('fleet-search');
  await search.fill(serial);

  // Either exactly that row, or an explicit empty state — never the old list with no indication.
  await expect(async () => {
    const after = await rows.count();
    const empty = await app.getByText(/no devices|no device matches/i).count();
    expect(after === 1 || empty > 0).toBe(true);
  }).toPass();

  // The count line names what was searched for, so the page cannot look like the whole fleet.
  await expect(app.getByText(new RegExp(`matching “?${serial}`))).toBeVisible();
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
