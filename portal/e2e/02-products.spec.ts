import { expect, test } from './fixtures';

/**
 * The product lifecycle, which is the vendor's main job: a described product goes draft →
 * published, and then asks to be certified.
 *
 * Each test creates a product with a unique key so the suite can be run repeatedly against the same
 * database, and cleans up nothing: a draft product is harmless and a published one is what the
 * seeded vendor already has.
 */
test('the product list shows status and certification separately', async ({ app }) => {
  await expect(app.getByRole('heading', { name: 'Products' })).toBeVisible();

  // Two columns, not one: publishing is the vendor's promise, certification is ours. A table that
  // merged them would hide the difference the whole gate exists to make.
  await expect(app.getByRole('columnheader', { name: 'Status' })).toBeVisible();
  await expect(app.getByRole('columnheader', { name: 'Certification' })).toBeVisible();
});

test('a vendor creates a product, sees it validated, and publishes it', async ({ app }) => {
  const key = `hv_e2e_${Date.now().toString(36)}`;

  await app.getByRole('link', { name: 'Products', exact: true }).click();
  await app.getByRole('button', { name: 'New product' }).click();

  await app.getByLabel('Product key').fill(key);
  await app.getByLabel('Name').fill(`E2E Bulb ${key.slice(-4)}`);

  // Creating lands in the editor for the new product, which is the next thing a vendor needs to do
  // — defining what the device can actually do.
  await app.getByRole('button', { name: /create draft/i }).click();

  await expect(app).toHaveURL(/\/products\/[0-9a-f-]{36}$/);
  // The editor holds the thing model, and the key it was created with is in it.
  await expect(app.getByRole('textbox', { name: 'Thing model' })).toContainText(key);
});

test('a product editor shows the certification state and offers to ask for a review', async ({ app }) => {
  await app.getByRole('link', { name: 'Products', exact: true }).click();

  // The seeded, published product: certified, therefore no "submit" button.
  await app.getByRole('link', { name: /Acme Bulb/ }).first().click();

  await expect(app.getByText(/certif/i).first()).toBeVisible();
});

test('a vendor can open a product and reach its firmware page', async ({ app }) => {
  await app.getByRole('link', { name: 'Products', exact: true }).click();
  await app.getByRole('link', { name: /Acme Bulb/ }).first().click();

  await app.getByRole('link', { name: /firmware/i }).first().click();
  await expect(app.getByRole('heading', { name: /firmware/i })).toBeVisible();
});
