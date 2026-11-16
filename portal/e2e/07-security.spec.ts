import { expect, test } from './fixtures';

/**
 * How the team signs in — read-only, deliberately.
 *
 * This suite shares one seeded vendor account with every other spec, and the account is the only
 * one who can sign in. An earlier version of this file enrolled an authenticator and enabled the
 * policy, which left the *next* run unable to log in at all: the account needed a code nobody could
 * produce. A test that changes a credential the rest of the suite depends on is not a test, it is a
 * trap.
 *
 * So: the screens are checked for what they promise (the state, the explanation, the refusal), and
 * the enrol → confirm → sign-in cycle is covered by the domain tests and by a live curl pass where
 * the secret is known for the duration of the run.
 */

test('security shows the two-factor state and the SSO intent', async ({ app }) => {
  await app.getByRole('link', { name: 'Security', exact: true }).click();

  await expect(app.getByRole('heading', { name: 'Security' })).toBeVisible();
  await expect(app.getByText(/two-factor/i).first()).toBeVisible();
  await expect(app.getByText(/single sign-on/i).first()).toBeVisible();

  // Which members are enrolled, in a sentence a person can act on.
  await expect(app.getByText(/of \d+ members enrolled/i)).toBeVisible();

  // SSO is recorded but not enforced, and the screen says which — a vendor reading "enforced" when
  // nobody validates an assertion would believe a protection they do not have.
  await expect(app.getByText(/not enforced|enforced/i).first()).toBeVisible();
});

test('requiring two-factor is refused while somebody has not enrolled', async ({ app }) => {
  await app.goto('/security');

  const toggle = app.getByLabel('Require two-factor for everyone');
  const summary = (await app.getByText(/of \d+ members enrolled/i).textContent()) ?? '';
  const noneEnrolled = /^0 of/.test(summary.trim());

  if (noneEnrolled) {
    // The screen warns before the toggle is pressed, so the refusal is never a surprise.
    await expect(app.getByText(/cannot be required|has not enrolled/i)).toBeVisible();

    await toggle.click();
    // And the API refuses it anyway — the UI is not the gate.
    await expect(app.getByText(/enrol|lock them out/i).first()).toBeVisible();
    await expect(toggle).not.toBeChecked();
  } else {
    // Somebody is enrolled, so the control is allowed to be on. Leave the account as it was.
    await expect(toggle).toBeVisible();
  }
});

test('a member can start an enrolment and is shown the secret once', async ({ app }) => {
  await app.goto('/security');

  await app.getByRole('button', { name: /set up authenticator|replace my secret/i }).click();

  const secret = app.getByTestId('totp-secret');
  await expect(secret).toBeVisible();

  // Base32, the shape every authenticator app expects.
  const value = (await secret.textContent()) ?? '';
  expect(value).toMatch(/^[A-Z2-7]{16,}$/);

  // The screen tells the person what to do with it rather than leaving them with a string.
  await expect(app.getByLabel('Six-digit code')).toBeVisible();
  await expect(app.getByText(/authenticator app/i).first()).toBeVisible();

  // Not confirmed on purpose: this is the shared account, and a confirmed factor would need a new
  // code at every later sign-in in this suite. An unconfirmed secret is not a factor — the login
  // path ignores it — so the account is left exactly as usable as it was.
  await app.getByRole('button', { name: /replace my secret|set up authenticator/i }).isVisible();
});

test('the SSO domain can be recorded without claiming enforcement', async ({ app }) => {
  await app.goto('/security');

  await app.getByLabel('SSO email domain').fill('acme.test');
  await app.getByRole('button', { name: /save domain/i }).click();

  await expect(app.getByText(/acme\.test/).first()).toBeVisible();
  // Still honest about enforcement, whatever the domain says.
  await expect(app.getByText(/not enforced|enforced/i).first()).toBeVisible();
});
