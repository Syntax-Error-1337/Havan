import { expect, test, type Page } from '@playwright/test';

/**
 * The journeys a customer walks on the first day, and the one they walk every day after.
 *
 * These are deliberately few and deliberately blunt. A deep test of a chart belongs in the unit suite,
 * where it can assert the arithmetic; what this file asserts is that each screen arrives with the
 * *customer's own data* on it, because "the app loads" and "the app loaded your home" are different
 * claims and only the second one is the product.
 */

const DEMO = {
  email: process.env.E2E_DEMO_EMAIL ?? 'demo@haven.local',
  password: process.env.E2E_DEMO_PASSWORD ?? 'haven12345',
};

/** Sign in through the real form, the way a person does. */
async function signIn(page: Page) {
  await page.goto('/');
  // The login screen is branded by the operator and renders before any session exists.
  // `exact` matters: the password field has a "Show password" control beside it, and a substring match
  // finds both — which is also a small lesson about accessible names that a real screen reader user
  // would hit.
  await page.getByLabel('Email', { exact: true }).fill(DEMO.email);
  await page.getByLabel('Password', { exact: true }).fill(DEMO.password);
  await page.getByRole('button', { name: 'Sign in', exact: true }).click();
  await expect(page.getByText('DASHBOARD')).toBeVisible();
}

test('the house is on the screen after signing in', async ({ page }) => {
  await signIn(page);

  // The home's own name, a device count that is a fact about the home, and at least one device tile.
  await expect(page.getByText('Workshop')).toBeVisible();
  await expect(page.getByText(/\d+ devices/)).toBeVisible();
  await expect(page.getByText(/Lights|Humidity|Sensor/).first()).toBeVisible();
});

test('a device opens with its readings and its room', async ({ page }) => {
  await signIn(page);

  // Through the UI, not by URL: a deep link that works is not the same as a tile that opens.
  await page.getByText('Room Sensor').first().click();
  // `.first()` in document order found the *dashboard's* "6 offline" pill, still mounted underneath the pushed
  // screen, and reported it hidden. The device screen's own status line is the thing under test.
  await expect(page.getByText(/· Living Room/)).toBeVisible();
  await expect(page.getByText('Offline — showing the last values this device reported.')).toBeVisible();
  await expect(page.getByText(/Temperature/).first()).toBeVisible();
  // The room row is at the bottom now, above the destructive action.
  await expect(page.getByText('In which room')).toBeVisible();

  // And the destructive action is *reachable*: this screen had no scroll container, and on a phone everything
  // below 844 points was simply cut off — the browser hid it by scrolling the whole document.
  const remove = page.getByTestId('remove-device');
  await remove.scrollIntoViewIfNeeded();
  await expect(remove).toBeInViewport();
  await expect(remove).toBeVisible();
});

test('a rule can be run by hand, and the run is recorded as a manual one', async ({ page }) => {
  await signIn(page);

  // A test that skips its own subject is a check nobody is really running: rather than assert an empty
  // state on a home with no rules, make one. It is deleted at the end, so the demo home is unchanged.
  const api = process.env.API_URL ?? 'http://127.0.0.1:8080';
  const token = await page.evaluate(() => localStorage.getItem('haven.access'));
  expect(token, 'the session token is what makes an API call possible from a browser test').toBeTruthy();
  const auth = { authorization: `Bearer ${token}` };

  const homes = await (await page.request.get(`${api}/v1/homes`, { headers: auth })).json();
  const homeId: string = homes[0].id;
  // A unique name, because the house already has rules of its own ("hook", "dusk") and `.first()` is not a
  // way to talk about the one this test made.
  const name = `E2E run-now ${Date.now()}`;
  const created = await page.request.post(`${api}/v1/homes/${homeId}/automations`, {
    headers: auth,
    data: {
      name,
      // A schedule, on purpose: a hand run of a scheduled rule is the case that used to be logged as
      // "schedule" and was the reason this endpoint was held back for a year.
      definition: {
        trigger: { type: 'schedule', every_secs: 86_400 },
        conditions: [],
        actions: [{ type: 'notify', title: 'E2E', body: 'run-now probe' }],
      },
    },
  });
  expect(created.status(), 'the rule is created for the test, not assumed').toBe(201);
  const ruleId: string = (await created.json()).id;

  try {
    await page.goto('/automations');

    // By the ids the screen already gives these controls. A text locator inside a 38-rule list was matching a
    // card by position rather than by identity, which is the same mistake `.first()` made, one layer down.
    await page.getByTestId(`run-${ruleId}`).click();

    // Two facts, checked separately, because they fail for different reasons.
    //
    // 1. The worker recorded the run as *manual* — not "schedule". This asserts against the API, so a slow worker
    //    makes the test wait rather than fail: the old version waited 25 s for a label that only appears after the
    //    worker wakes, which is why it failed in roughly half of full runs and passed alone.
    await expect
      .poll(
        async () => {
          const response = await page.request.get(`${api}/v1/automations/${ruleId}/runs`, { headers: auth });
          if (!response.ok()) return null;
          const runs = (await response.json()) as { trigger_kind?: string }[];
          return runs[0]?.trigger_kind ?? null;
        },
        { timeout: 45_000, intervals: [500, 1_000, 2_000] },
      )
      .toBe('manual');

    // 2. The screen shows it. The sheet is opened *after* the run exists, because a list fetched before the run was
    //    written is an empty list — correct behaviour, and not something a longer wait can fix.
    await page.getByTestId(`history-${ruleId}`).click();
    await expect(page.getByText(/manual/i).first()).toBeVisible({ timeout: 15_000 });
  } finally {
    await page.request.delete(`${api}/v1/automations/${ruleId}`, { headers: auth });
  }
});

test('appearance follows the customer, not only the operator', async ({ page }) => {
  await signIn(page);
  await page.goto('/settings');
  await expect(page.getByText('Appearance', { exact: true })).toBeVisible();

  await page.getByText('Light', { exact: true }).click();
  // A light palette paints a light surface; the assertion is the stored preference plus the visible
  // chip, because "did it get lighter?" is not a question a test can ask.
  const stored = await page.evaluate(() => localStorage.getItem('haven.appearance'));
  expect(stored).toBe('light');

  // Put it back: the next test (and the next person) expects the operator's default.
  await page.getByText('Match my phone').click();
  await expect
    .poll(async () => page.evaluate(() => localStorage.getItem('haven.appearance')))
    .toBe('system');
});

test('signing out returns to the login screen, not to an empty shell', async ({ page }) => {
  await signIn(page);
  await page.goto('/settings');
  await page.getByText('Sign out').click();

  await expect(page.getByLabel('Email')).toBeVisible();
  await expect(page.getByText('DASHBOARD')).toHaveCount(0);
});
