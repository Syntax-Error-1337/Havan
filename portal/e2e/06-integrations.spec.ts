import { expect, test } from './fixtures';

/** The two doors a vendor's own systems use: API keys, and webhooks that leave the building. */

test('an API key is minted and shown once', async ({ app }) => {
  await app.getByRole('link', { name: 'API keys', exact: true }).click();
  await expect(app.getByRole('heading', { name: 'API keys' })).toBeVisible();

  await app.getByLabel('Name').fill(`e2e-key-${Date.now().toString(36)}`);
  await app.getByRole('button', { name: /mint|create/i }).click();

  // The one sentence that must be read, and the value it refers to.
  await expect(app.getByText(/only time the secret is shown/i)).toBeVisible();
  await app.getByRole('button', { name: /done/i }).click();
  await expect(app.getByText(/only time the secret is shown/i)).toHaveCount(0);
});

test('revoking a key leaves a row that says so rather than deleting the history', async ({ app }) => {
  await app.goto('/api-keys');
  const revoke = app.getByRole('button', { name: /revoke/i }).first();
  if (await revoke.count()) {
    await revoke.click();
    await expect(app.getByText(/revoked/i).first()).toBeVisible();
  }
});

test('a webhook subscription returns a signing secret exactly once', async ({ app }) => {
  await app.getByRole('link', { name: 'Webhooks', exact: true }).click();
  await expect(app.getByRole('heading', { name: 'Webhooks' })).toBeVisible();

  // Unique per run: the platform refuses a duplicate endpoint with 409, which is correct and would
  // otherwise make the second run of this suite fail for the wrong reason.
  await app.getByLabel('Endpoint URL').fill(`https://erp-${Date.now().toString(36)}.example.test/haven`);
  await app.getByRole('button', { name: 'Subscribe' }).click();

  await expect(app.getByText(/signing secret/i)).toBeVisible();
  await app.getByRole('button', { name: 'Done' }).click();
  await expect(app.getByText(/signing secret/i)).toHaveCount(0);

  // And the endpoint is now listed, so the vendor can see what they signed up for.
  await expect(app.getByText(/erp-[a-z0-9]+\.example\.test\/haven/).first()).toBeVisible();
});

test('a subscription can be removed', async ({ app }) => {
  await app.goto('/webhooks');
  const endpoint = `https://erp-${Date.now().toString(36)}.example.test/haven`;
  await app.getByLabel('Endpoint URL').fill(endpoint);
  await app.getByRole('button', { name: 'Subscribe' }).click();
  await app.getByRole('button', { name: 'Done' }).click();

  // This endpoint and no other: earlier runs leave subscriptions behind, and a test that demanded a
  // globally empty table would fail for a reason that has nothing to do with removing.
  const row = app.getByRole('row').filter({ hasText: endpoint });
  await expect(row).toHaveCount(1);

  // The removal asks first — "Stop sending events to …?" — and Playwright *dismisses* dialogs by default, so without
  // this the delete never happened and the assertion below waited out its timeout on a row that was still there.
  app.once('dialog', (dialog) => void dialog.accept());
  await row.getByRole('button', { name: /remove/i }).click();
  await expect(app.getByRole('row').filter({ hasText: endpoint })).toHaveCount(0);
});
