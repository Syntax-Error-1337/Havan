import { expect, type Page, test as base } from '@playwright/test';

/**
 * The seeded vendor account from `devseed`.
 *
 * Overridable by environment so the suite can point at a staging vendor, but defaulting to the
 * development seed keeps `npm run test:e2e` a single command on a laptop.
 */
export const VENDOR = {
  /** What the sign-in form asks for: the workspace's short name. */
  slug: process.env.E2E_VENDOR_SLUG ?? 'acme',
  /** The id, for the admin console, which works in ids. */
  id: process.env.E2E_VENDOR_ID ?? '11111111-1111-7111-8111-111111111111',
  email: process.env.E2E_VENDOR_EMAIL ?? 'admin@acme.test',
  password: process.env.E2E_VENDOR_PASSWORD ?? 'acme-demo-password',
};

/** Sign in through the real form, the way a vendor does. */
export async function signIn(page: Page) {
  await page.goto('/login');
  await page.getByLabel('Workspace').fill(VENDOR.slug);
  await page.getByLabel('Email').fill(VENDOR.email);
  await page.getByLabel('Password').fill(VENDOR.password);
  await page.getByRole('button', { name: 'Sign in' }).click();

  // The product list is the landing page; waiting for it is what proves the token was accepted.
  await expect(page.getByRole('heading', { name: 'Products' })).toBeVisible();
}

/**
 * A page that is already signed in.
 *
 * Every spec gets this, because a vendor's first minute is signing in and every later minute is
 * work — a suite that repeats the login five times spends its time on the part least likely to be
 * broken.
 */
export const test = base.extend<{ app: Page }>({
  app: async ({ page }, use) => {
    await signIn(page);
    await use(page);
  },
});

export { expect };
