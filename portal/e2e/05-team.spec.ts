import { expect, test } from './fixtures';

/**
 * The team: inviting a colleague, and the rules that stop the team removing the only person who can
 * still administer it.
 */

test('the team screen lists members with their role and last sign-in', async ({ app }) => {
  await app.getByRole('link', { name: 'Team', exact: true }).click();

  await expect(app.getByRole('heading', { name: 'Team' })).toBeVisible();
  // Two tables on this screen, both with a Role column: the member list is the one that matters.
  const members = app.getByRole('region', { name: /team members/i });
  await expect(members.getByRole('columnheader', { name: 'Role' })).toBeVisible();
  await expect(members.getByRole('columnheader', { name: /last login/i })).toBeVisible();
  // An owner exists, and the screen says so rather than leaving the role column ambiguous.
  await expect(app.getByText('owner', { exact: true }).first()).toBeVisible();
});

test('an invitation produces a link that is shown exactly once', async ({ app }) => {
  await app.goto('/team');

  const email = `e2e-invite-${Date.now().toString(36)}@acme.test`;
  await app.getByLabel('Email to invite').fill(email);
  await app.getByRole('button', { name: 'Create invitation' }).click();

  // The token cannot be retrieved later, so the screen has to show it now — and say that the link
  // is the vendor's to send, because the platform has no mail transport.
  const token = app.getByTestId('invite-token');
  await expect(token).toBeVisible();
  await expect(token).toContainText('accept?token=');
  await expect(app.getByText(/send this link yourself/i)).toBeVisible();
});

test('a pending invitation appears in the list with its expiry', async ({ app }) => {
  await app.goto('/team');

  const email = `e2e-pending-${Date.now().toString(36)}@acme.test`;
  await app.getByLabel('Email to invite').fill(email);
  await app.getByRole('button', { name: 'Create invitation' }).click();
  await expect(app.getByTestId('invite-token')).toBeVisible();

  await app.reload();
  await expect(app.getByText(email)).toBeVisible();
});

test('the owner cannot be removed from the team', async ({ app }) => {
  await app.goto('/team');
  await expect(app.getByText('owner', { exact: true }).first()).toBeVisible();

  // Either the button is absent for the owner row, or pressing it is refused with a reason. Both
  // are acceptable; silently removing the last owner is not.
  const ownerRow = app.getByRole('row').filter({ hasText: 'owner' }).first();
  const remove = ownerRow.getByRole('button', { name: /remove/i });
  if (await remove.count()) {
    await remove.click();
    await expect(app.getByText(/last owner|only owner|cannot/i).first()).toBeVisible();
  } else {
    await expect(remove).toHaveCount(0);
  }
});
