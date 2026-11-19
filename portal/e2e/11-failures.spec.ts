import { expect, test } from './fixtures';

/**
 * What a vendor reads when a screen does not load.
 *
 * Every page used to print `error.message`, which is correct when the server answered — its sentences are written for a
 * person — and wrong when the request never arrived, where it surfaced as `TypeError: Failed to fetch`. A support
 * engineer cannot act on that, and a vendor cannot tell whether their click was applied.
 *
 * These two tests are the two halves of that distinction, driven through the real page with the network intercepted.
 */

test('a request that never arrived says so, and says nothing was changed', async ({ app }) => {
  await app.route('**/vendor/devices*', (route) => route.abort('failed'));
  await app.goto('/fleet');

  await expect(
    app.getByText('The portal could not reach the platform. Check your connection — nothing was changed.'),
  ).toBeVisible();
  // And there is a way out of the state, rather than a dead end.
  await expect(app.getByRole('button', { name: 'Try again' })).toBeVisible();
});

test('a refusal from the server is repeated, because it was written for a person', async ({ app }) => {
  await app.route('**/vendor/devices*', (route) =>
    route.fulfill({
      status: 403,
      contentType: 'application/json',
      body: JSON.stringify({
        error: { code: 'forbidden', message: 'only an owner can view the fleet' },
      }),
    }),
  );
  await app.goto('/fleet');

  await expect(app.getByText('only an owner can view the fleet')).toBeVisible();
  // Not the client's placeholder, and not a status code.
  await expect(app.getByText(/Request failed \(403\)/)).toHaveCount(0);
});
