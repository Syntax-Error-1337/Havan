import { fireEvent, render, waitFor } from '@testing-library/react-native';
import { QueryClientProvider } from '@tanstack/react-query';
import { testQueryClient } from '../../test/query';

import { PrivacyPage } from './PrivacyPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';
// The save itself is platform I/O (an anchor on web, a file plus share sheet on a phone); the page's job
// is to ask for the bundle and report where it went, so only the saving is replaced.
jest.mock('../../entities/privacy', () => ({
  ...jest.requireActual('../../entities/privacy'),
  saveExport: jest.fn(async () => 'haven-export-20261117.json'),
}));

/**
 * The two rights, on screen.
 *
 * A request that is never sent is the failure mode worth guarding: both buttons must reach the API, and
 * deleting must be impossible without the person confirming an explicit sentence.
 */

const bundle = {
  format_version: 1,
  exported_at: '2026-11-17T09:20:11Z',
  user: { id: 'u1' },
  homes: [{}],
  notifications: [],
  telemetry: [{}, {}],
  notes: [],
};

const receipt = {
  homes_deleted: 1,
  devices_deleted: 2,
  memberships_removed: 1,
  notifications_deleted: 0,
  sessions_revoked: 1,
  push_tokens_removed: 1,
  retention_days: 30,
  purge_after: '2026-12-17T00:00:00Z',
};

function stubApi(handler: (url: string, init?: RequestInit) => unknown) {
  const calls: { url: string; method: string }[] = [];
  globalThis.fetch = jest.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, method: init?.method ?? 'GET' });
    const body = handler(url, init);
    return {
      ok: true,
      status: 200,
      text: async () => JSON.stringify(body ?? null),
      headers: { get: () => null },
    } as unknown as Response;
  }) as unknown as typeof fetch;
  return calls;
}

async function renderPage() {
  return await render(
    <QueryClientProvider client={testQueryClient()}>
      <ThemeProvider>
        <PrivacyPage />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  useSession.setState({ access: 'token', homeId: 'home-1', refresh: null });
});

describe('privacy', () => {
  it('asks the API for the export and says what was saved', async () => {
    const calls = stubApi((url) => (url.includes('/me/export') ? bundle : null));
    const view = await renderPage();

    fireEvent.press(view.getByTestId('export-data'));

    await waitFor(() => expect(calls.some((c) => c.url.includes('/me/export'))).toBe(true));
    // Web downloads through an anchor; the file name and the counts are the visible result either way.
    expect(await view.findByTestId('export-result')).toBeTruthy();
    expect(view.getByText(/1 home · 2 readings · 0 notifications/)).toBeTruthy();
  });

  it('does not delete anything until the person confirms', async () => {
    const calls = stubApi(() => receipt);
    const view = await renderPage();

    fireEvent.press(view.getByTestId('delete-account'));

    expect(await view.findByText('Delete your account?')).toBeTruthy();
    expect(calls.some((c) => c.method === 'DELETE')).toBe(false);

    // Keeping it is a real choice, and leaves no trace on the server.
    fireEvent.press(view.getByTestId('confirm-cancel'));
    await waitFor(() => expect(view.queryByText('Delete your account?')).toBeNull());
    expect(calls.some((c) => c.method === 'DELETE')).toBe(false);
  });

  it('deletes the account, then signs out and shows the receipt', async () => {
    const calls = stubApi((url, init) =>
      init?.method === 'DELETE' ? receipt : bundle,
    );
    const signOut = jest.fn(async () => undefined);
    useSession.setState({ signOut });

    const view = await renderPage();
    fireEvent.press(view.getByTestId('delete-account'));
    await view.findByText('Delete your account?');
    fireEvent.press(view.getByTestId('confirm-accept'));

    await waitFor(() =>
      expect(calls.some((c) => c.method === 'DELETE' && c.url.endsWith('/me'))).toBe(true),
    );

    // The receipt comes before the sign-out: signing out first unmounts this screen through the session
    // gate, and the person is left at a login form with no idea whether anything happened.
    expect(await view.findByText('Your account is gone')).toBeTruthy();
    expect(view.getByText(/1 home and 2 devices deleted/)).toBeTruthy();
    expect(signOut).not.toHaveBeenCalled();

    fireEvent.press(view.getByTestId('confirm-accept'));
    // The server has already revoked the refresh token; clearing the local session is what stops the app
    // from looking signed in to an account that no longer exists.
    await waitFor(() => expect(signOut).toHaveBeenCalled());
  });
});
