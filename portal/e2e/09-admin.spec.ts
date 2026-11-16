import { expect, test } from '@playwright/test';

/**
 * The operator console.
 *
 * `docs/11` writes one promise about suspension, and a lawyer reads it first: new activations stop,
 * existing devices keep working, the portal goes read-only. These tests check the parts a browser can
 * see — the switch, the badge, and the sentence that says what it did — while the API tests own the
 * claim refusal.
 *
 * The console needs no token on loopback, which is where this suite runs; a token-protected
 * deployment is covered by the same `x-ops-token` header the API tests exercise.
 */

test.beforeEach(async ({ page }) => {
  await page.goto('/admin');
  await expect(page.getByRole('heading', { name: 'Vendors' })).toBeVisible();
});

test('the console opens on the operator side, not the vendor one', async ({ page }) => {
  // The vendor login is what a portal shows; the console must never ask a vendor for a password to
  // reach a screen that manages vendors.
  await expect(page.getByText('Operator console')).toBeVisible();
  await expect(page.getByLabel('Ops token')).toHaveCount(0);

  // And the four numbers a person opens this screen to see.
  await expect(page.getByTestId('admin-overview')).toContainText(/vendors/);
  await expect(page.getByTestId('admin-overview')).toContainText(/awaiting certification/);
});

test('every vendor is listed with what they have live', async ({ page }) => {
  const table = page.getByRole('region', { name: 'Vendors' });
  await expect(table.getByRole('columnheader', { name: 'Status' })).toBeVisible();
  await expect(table.getByRole('columnheader', { name: 'Devices' })).toBeVisible();
  await expect(table.getByRole('columnheader', { name: 'Licences' })).toBeVisible();

  // The seeded vendor is a known row, so the table is checked against a fact rather than a count.
  await expect(page.getByTestId('vendor-acme')).toBeVisible();
});

test('opening a vendor answers the phone call', async ({ page }) => {
  // The seeded vendor is the one with data: products, devices and a ledger.
  await page.getByTestId('vendor-acme').getByRole('link').first().click();

  await expect(page.getByTestId('vendor-heading')).toHaveText('Acme Devices');
  // Balance, and the activations behind it — the two questions a vendor asks first.
  await expect(page.getByText('Granted').first()).toBeVisible();
  await expect(page.getByText('Remaining').first()).toBeVisible();
  await expect(page.getByTestId('vendor-invoice')).toContainText(/draft|issued/);
  await expect(page.getByRole('columnheader', { name: 'Serial' })).toBeVisible();
});

test('a vendor is onboarded from the console', async ({ page }) => {
  const slug = `e2e-${Date.now().toString(36)}`;

  await page.getByRole('button', { name: 'Onboard vendor' }).click();
  await page.getByLabel('Vendor slug').fill(slug);
  await page.getByLabel('Vendor name').fill('E2E Vendor');
  await page.getByRole('button', { name: 'Create vendor' }).click();

  const row = page.getByTestId(`vendor-${slug}`);
  await expect(row).toBeVisible();
  await expect(row).toContainText('E2E Vendor');
  await expect(row).toContainText('active');
});

test('suspending a vendor is one press and says what it means', async ({ page }) => {
  const slug = `sus-${Date.now().toString(36)}`;
  await page.getByRole('button', { name: 'Onboard vendor' }).click();
  await page.getByLabel('Vendor slug').fill(slug);
  await page.getByLabel('Vendor name').fill('To Suspend');
  await page.getByRole('button', { name: 'Create vendor' }).click();

  const row = page.getByTestId(`vendor-${slug}`);
  await row.getByRole('button', { name: 'Suspend' }).click();

  await expect(row.getByText('suspended')).toBeVisible();
  await expect(row.getByRole('button', { name: 'Restore' })).toBeVisible();

  // The promise, in the screen's own words: devices stay, activations stop.
  await expect(page.getByText(/devices already in people's homes keep working/i)).toBeVisible();
  await expect(page.getByTestId('suspension-note')).toContainText(/no new activations/i);

  // And restoring puts it back.
  await row.getByRole('button', { name: 'Restore' }).click();
  await expect(row.getByText('active')).toBeVisible();
});

test('the certification queue is where a submitted product waits for us', async ({ page }) => {
  await page.getByRole('link', { name: 'Certification' }).click();

  await expect(page.getByRole('heading', { name: 'Certification queue' })).toBeVisible();
  const table = page.getByRole('region', { name: 'Awaiting certification' });
  await expect(table.getByRole('columnheader', { name: 'Product' })).toBeVisible();

  // Either something is waiting and it can be signed off, or the empty state says so plainly.
  const certify = page.getByRole('button', { name: 'Certify' });
  const empty = page.getByText(/nothing waiting/i);
  expect((await certify.count()) + (await empty.count())).toBeGreaterThan(0);

  // And the row says how to verify before signing: a command an operator can paste, not a promise
  // that somebody tested it.
  if (await certify.count()) {
    await expect(page.getByText(/make conformance VENDOR=/).first()).toBeVisible();
    await expect(page.getByText(/Run the command before certifying/i)).toBeVisible();
  }
});

test('billing grants licences and issues a month', async ({ page }) => {
  await page.getByRole('link', { name: 'Billing' }).click();
  await expect(page.getByRole('heading', { name: 'Billing' })).toBeVisible();

  // The vendor select is populated from the same list the first tab shows.
  await expect(page.getByLabel('Vendor')).toContainText('Acme');

  await page.getByLabel('Quantity').fill('5');
  await page.getByLabel('Unit price in cents').fill('199');
  await page.getByRole('button', { name: 'Grant' }).click();
  await expect(page.getByRole('button', { name: 'Grant' })).toBeEnabled();

  // Issuing the current month is idempotent on the server, so repeating it is safe and the answer is
  // always the same document.
  await page.getByRole('button', { name: 'Issue' }).click();
  await expect(page.getByTestId('issued-invoice')).toBeVisible();
  await expect(page.getByTestId('issued-invoice')).toContainText(/issued|draft/);
});

test('health answers "is anything talking to us" without interpreting a log', async ({ page }) => {
  await page.getByRole('link', { name: 'Health' }).click();

  await expect(page.getByRole('heading', { name: 'Health' })).toBeVisible();
  // The tile labels repeat inside the offender tables, so each is matched once.
  await expect(page.getByText('Devices', { exact: true }).first()).toBeVisible();
  // The two numbers that disagree when a device holds a stale connection — the fault the platform's
  // own `online` flag used to hide by never clearing it.
  await expect(page.getByText('Heard in 15 min')).toBeVisible();
  await expect(page.getByText('Undelivered commands').first()).toBeVisible();

  // A count nobody can act on is a count that gets ignored: the screen names the devices that have
  // been quiet longest, and the commands waiting to be sent.
  await expect(page.getByText('Quietest devices')).toBeVisible();
  await expect(page.getByText('Commands we could not send')).toBeVisible();
});

test('the console fits a phone viewport', async ({ page }) => {
  await page.setViewportSize({ width: 420, height: 900 });
  await page.goto('/admin');
  await page.waitForLoadState('networkidle');

  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow, 'the console scrolls sideways at 420 px').toBeLessThanOrEqual(1);
});
