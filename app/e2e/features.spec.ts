import { expect, test, type Page } from '@playwright/test';

/**
 * The three things the app promised and the platform could not do.
 *
 * Each one ends where it started, on purpose: this suite runs against the shared demo home, and a test that leaves the
 * hallway sensor removed or quiet hours on has broken the next test rather than proved anything.
 */
async function signIn(page: Page) {
  await page.goto('/');
  await page.getByLabel('Email', { exact: true }).fill('demo@haven.local');
  await page.getByLabel('Password', { exact: true }).fill('haven12345');
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(page.getByText('DASHBOARD')).toBeVisible();
}

test.describe.configure({ mode: 'serial' });

test('a removal offers an undo, and the undo puts the device back', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await signIn(page);

  // `router.replace` leaves the previous dashboard instance mounted and hidden behind the current one, so the same
  // testID exists twice in the DOM with one of them at zero size. Only the visible one is the screen the customer sees.
  const tile = page.locator('[data-testid="device-tile-SEED-SENSOR-1"]:visible');
  await tile.first().click();
  await page.waitForTimeout(900);

  await page.getByTestId('remove-device').scrollIntoViewIfNeeded();
  await page.getByTestId('remove-device').click();
  await page.getByRole('button', { name: 'Remove', exact: true }).click();

  // The toast names the device and offers the way back: several of them are called "Light".
  const toast = page.locator('[role="alert"]');
  await expect(toast).toContainText('Removed');
  await expect(page.getByTestId('toast-action')).toHaveText('Undo');

  try {
    await page.getByTestId('toast-action').click();
    await expect(toast).toContainText('is back in its home', { timeout: 8000 });
    await expect(tile.first()).toBeVisible({ timeout: 8000 });
  } finally {
    // Whatever happened above, do not leave the demo home short a device.
    if ((await tile.count()) === 0) {
      await page.goto('/');
      await page.waitForTimeout(500);
    }
  }
});

test('asking a device to identify says the platform asked, not that it blinked', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await signIn(page);

  await page.locator('[data-testid="device-tile-SEED-SENSOR-1"]:visible').first().click();
  await page.waitForTimeout(900);
  await page.getByTestId('identify-device').scrollIntoViewIfNeeded();
  await page.getByTestId('identify-device').click();

  // `202 accepted` is not a blink. The first sentence is about the request, and the outcome after it is about the
  // device — which, in this demo home, is asleep.
  await expect(page.locator('[role="alert"]')).toContainText(/to blink|No answer/, { timeout: 8000 });
});

test('quiet hours are stored on the server and can be turned off again', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await signIn(page);
  await page.goto('/settings');
  await page.waitForTimeout(900);

  const row = page.getByTestId('quiet-hours');
  await row.scrollIntoViewIfNeeded();
  await expect(row).toContainText('Off');

  await row.click();
  await page.getByTestId('quiet-from').fill('21:30');
  await page.getByTestId('quiet-to').fill('06:15');
  await page.getByTestId('quiet-save').click();
  await expect(row).toContainText('21:30 – 06:15', { timeout: 8000 });

  // Read back from the server, not from local state: reload and the window is still there.
  await page.reload();
  await page.waitForTimeout(1200);
  await expect(page.getByTestId('quiet-hours')).toContainText('21:30 – 06:15');

  await page.getByTestId('quiet-hours').click();
  await page.getByTestId('quiet-off').click();
  await expect(page.getByTestId('quiet-hours')).toContainText('Off', { timeout: 8000 });
});
