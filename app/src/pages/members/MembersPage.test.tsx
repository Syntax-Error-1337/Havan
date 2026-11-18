import { act, fireEvent, render, screen, waitFor } from '@testing-library/react-native';
import { QueryClientProvider } from '@tanstack/react-query';
import { testQueryClient } from '../../test/query';

import { MembersPage } from './MembersPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';

/**
 * The people in the home.
 *
 * The screen exists because the app showed a count — "2 members" — and a way to change neither, while the API had
 * served the list, invitations, role changes and removals all along. What these tests hold is the two things a
 * member list must never get wrong: **the owner cannot be edited** (ownership transfer is its own operation), and
 * a person without the role sees no control that would 403.
 */

const home = {
  id: 'home-1',
  owner_id: 'u-owner',
  name: 'Flat',
  timezone: 'UTC',
  role: 'owner',
  member_count: 3,
  device_count: 6,
  created_at: '2026-01-01T00:00:00Z',
};

const members = [
  { user_id: 'u-owner', display_name: 'Ada Lovelace', email: 'ada@example.com', role: 'owner', can_pair: true },
  { user_id: 'u-me', display_name: 'Grace Hopper', email: 'grace@example.com', role: 'admin', can_pair: true },
  { user_id: 'u-guest', display_name: 'Alan Turing', email: null, role: 'guest', can_pair: false },
];

function stubApi(over: { home?: unknown; members?: unknown[] } = {}) {
  const calls: { url: string; method: string; body: unknown }[] = [];
  globalThis.fetch = jest.fn(async (url: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET';
    const body = typeof init?.body === 'string' ? JSON.parse(init.body) : null;
    calls.push({ url: String(url), method, body });

    let payload: unknown = [];
    if (String(url).endsWith('/homes')) payload = [over.home ?? home];
    else if (String(url).includes('/members') && method === 'POST') {
      // The invite endpoint now answers with what it did: `added` for an existing account, `invited` plus a token for an
      // address with none. A 204 here would be a server this app no longer talks to.
      payload = { outcome: 'added' };
    } else if (String(url).includes('/members')) payload = over.members ?? members;
    else if (String(url).includes('/invitations')) payload = [];
    else if (String(url).endsWith('/me')) payload = { id: 'u-me', email: 'grace@example.com', display_name: 'Grace Hopper', locale: 'en' };

    return {
      ok: true,
      status: method === 'POST' ? 204 : 200,
      text: async () => (payload === undefined ? '' : JSON.stringify(payload)),
    } as Response;
  }) as unknown as typeof fetch;
  return calls;
}

async function renderPage() {
  return await render(
    <QueryClientProvider client={testQueryClient()}>
      <ThemeProvider>
        <MembersPage />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  useSession.setState({ access: 'token', refresh: 'r', homeId: 'home-1', hydrated: true });
});

describe('members', () => {
  it('names the people, their roles, and which one is you', async () => {
    stubApi();
    await renderPage();

    expect(await screen.findByText('Ada Lovelace')).toBeTruthy();
    expect(screen.getByText('Alan Turing')).toBeTruthy();
    expect(screen.getByText('Grace Hopper (you)')).toBeTruthy();
    expect(screen.getByText('Owner')).toBeTruthy();
    // "Guest" is both Alan's role and one of the invitation chips; the row's own label is what a screen reader
    // reads, and it is unambiguous.
    expect(screen.getByLabelText('Role for Alan Turing: Guest')).toBeTruthy();
  });

  it('offers no role control for the owner — transfer is a separate operation', async () => {
    stubApi();
    await renderPage();

    await screen.findByText('Ada Lovelace');
    // The owner's role is text; only the *other* people have a control.
    expect(screen.queryByLabelText(/Role for Ada Lovelace/)).toBeNull();
    expect(screen.getByLabelText('Role for Alan Turing: Guest')).toBeTruthy();
    // And never your own row: an admin demoting themselves by accident is not a feature.
    expect(screen.queryByLabelText(/Role for Grace Hopper/)).toBeNull();
  });

  it('adds a person who already has an account, with the role chosen', async () => {
    const calls = stubApi();
    await renderPage();

    // The field's state has to be applied before the button is pressed: "Add to home" is disabled without an
    // address, and a press on a disabled control is a press that does nothing at all.
    const input = await screen.findByLabelText('Email address');
    await act(async () => {
      fireEvent.changeText(input, 'alan@example.com');
    });
    // The invite picker's chips carry their own labels — "Admin" alone also matches an existing member's role.
    await act(async () => {
      fireEvent.press(screen.getByLabelText('Role: Admin'));
    });
    await act(async () => {
      fireEvent.press(screen.getByText('Add to home'));
    });

    await waitFor(() => {
      const post = calls.find((c) => c.method === 'POST');
      expect(post).toBeTruthy();
      expect(post?.url).toContain('/homes/home-1/members');
      expect(post?.body).toEqual({ email: 'alan@example.com', role: 'admin' });
    });
  });

  it('shows a member no invitation form and no leave row for the owner', async () => {
    stubApi({ home: { ...home, role: 'member', owner_id: 'u-owner' } });
    await renderPage();

    await screen.findByText('Ada Lovelace');
    expect(screen.queryByText('Add someone')).toBeNull();
    expect(screen.queryByText('Add to home')).toBeNull();
    // A member may still leave — that has always been allowed to anyone but the owner.
    expect(screen.getByText('Leave this home')).toBeTruthy();
  });

  it('does not offer the owner a way out of their own home', async () => {
    stubApi();
    await renderPage();

    await screen.findByText('Ada Lovelace');
    expect(screen.queryByText('Leave this home')).toBeNull();
  });
});
