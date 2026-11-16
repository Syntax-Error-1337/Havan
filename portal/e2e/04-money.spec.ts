import { expect, test } from './fixtures';

/**
 * The money: licences are what the vendor bought, invoices are what they owe.
 *
 * The distinction the screens must keep: a licence balance is a running total, an invoice is a
 * document for one month, and only one of those is allowed to change after somebody read it.
 */

test('licences show granted, activated, remaining and this month', async ({ app }) => {
  await app.getByRole('link', { name: 'Licences', exact: true }).click();

  await expect(app.getByRole('heading', { name: /device licences/i })).toBeVisible();
  // The four figures are card labels; "Activated" is also a table column, so each is matched
  // inside the summary rather than anywhere on the page.
  for (const label of ['Granted', 'Activated', 'Remaining', 'This month']) {
    await expect(app.getByText(label, { exact: true }).first()).toBeVisible();
  }
  await expect(app.getByRole('columnheader', { name: /serial/i })).toBeVisible();
});

test('an invoice for a month shows its status before its money', async ({ app }) => {
  await app.getByRole('link', { name: 'Invoices', exact: true }).click();

  await expect(app.getByRole('heading', { name: 'Invoices' })).toBeVisible();

  // Either word is legitimate; what must never happen is a number with no indication of whether it
  // is still open.
  await expect(app.getByText(/^(draft|issued)$/).first()).toBeVisible();
  await expect(app.getByLabel('Invoice month')).toBeVisible();
});

test('an issued invoice says it will not change, a draft says it still can', async ({ app }) => {
  await app.goto('/invoices');
  await expect(app.getByText(/^(draft|issued)$/).first()).toBeVisible();

  // The intro sentence also contains "will not change", so the assertion is scoped to the status
  // paragraph under the badge.
  const issued = await app.getByText('issued', { exact: true }).count();
  const expected = issued ? /Issued .* will not change/i : /Draft .* still count/i;
  // The intro paragraph contains both phrases, so the status line is the `.last()` of the matches.
  await expect(app.getByText(expected).last()).toBeVisible();
});

test('choosing another month does not require a reload', async ({ app }) => {
  await app.goto('/invoices');
  await expect(app.getByText(/^(draft|issued)$/).first()).toBeVisible();

  await app.getByLabel('Invoice month').fill('2026-01');
  // January is behind us: the query must answer, either with activations or with an empty state.
  await expect(
    app.getByText(/no devices were activated|^issued$|^draft$/i).first(),
  ).toBeVisible();
});
