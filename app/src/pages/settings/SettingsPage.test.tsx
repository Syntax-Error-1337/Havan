import { render, fireEvent, waitFor, within } from '@testing-library/react-native';
import { QueryClientProvider } from '@tanstack/react-query';
import { Alert } from 'react-native';

import { testQueryClient } from '../../test/query';
import { SettingsPage } from './SettingsPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';
import { usePreferences } from '../../entities/preferences';

const home = {
  id: 'home-1',
  name: 'Demo flat',
  timezone: 'UTC',
  owner_id: 'u1',
  role: 'owner',
  member_count: 1,
  device_count: 1,
  created_at: '2026-01-01T00:00:00Z',
};

const room = { id: 'room-1', home_id: 'home-1', name: 'Kitchen', icon: null, sort: 0 };

function stubApi() {
  const calls: { url: string; method: string; body?: string }[] = [];
  globalThis.fetch = jest.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? 'GET';
    calls.push({ url, method, body: init?.body as string });

    const body = (() => {
      if (url.endsWith('/me')) return { id: 'u1', email: 'demo@haven.dev', display_name: 'Demo', locale: 'en' };
      if (url.endsWith('/homes')) return [home];
      // An empty page with a real total: the shape that made this screen say "Devices 0".
      if (url.includes('/dashboard'))
        return { home, rooms: [room], devices: [], devices_total: 1, scenes: [], unread_count: 0 };
      if (url.endsWith('/rooms')) return [room];
      return null;
    })();

    return { ok: true, status: body === null ? 204 : 200, text: async () => JSON.stringify(body) } as Response;
  }) as unknown as typeof fetch;
  return calls;
}

async function renderPage() {
  const client = testQueryClient();
  return await render(
    <QueryClientProvider client={client}>
      <ThemeProvider>
        <SettingsPage />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  useSession.setState({
    access: 'token',
    refresh: 'refresh',
    homeId: 'home-1',
    hydrated: true,
  });
});

describe('appearance', () => {
  it('lets the customer overrule the operator about light and dark, and remembers it', async () => {
    // The palette has had a light half since the first theme pass and nothing could reach it: the
    // comment in `providers.tsx` claimed Settings could switch modes, and no such control existed.
    stubApi();
    useSession.setState({ access: 'token', homeId: 'home-1', refresh: null });

    const view = await renderPage();
    expect(await view.findByText('Match my phone')).toBeTruthy();
    expect(view.getByText('Dark')).toBeTruthy();

    fireEvent.press(view.getByText('Light'));

    // The store is the contract the provider reads, so asserting it is asserting the repaint.
    await waitFor(() => expect(usePreferences.getState().appearance).toBe('light'));
  });
});

describe('settings', () => {
  it('shows who is signed in', async () => {
    stubApi();
    const view = await renderPage();
    expect(await view.findByText('demo@haven.dev', {}, { timeout: 5000 })).toBeTruthy();
    expect(await view.findByText('Demo flat', {}, { timeout: 5000 })).toBeTruthy();
  });

  // The row said "Devices 0" on a six-device home: it counted `dashboard.devices`, which is one page of a
  // cursor and often none of it. The total is the only count that is true however much was fetched.
  it('counts every device in the home, not the page it happened to load', async () => {
    // The dashboard here answers with no devices and a total of one — the shape that produced
    // "Devices 0" on a home that had them. The count has to come from the total.
    const view = await renderPage();

    expect(await view.findByText('Devices')).toBeTruthy();
    // The count lands after the row draws (it waits for one of two answers), so this waits for it too.
    // Scoped to the row: "1" is also the member count now, and two rows that each say 1 are not the same claim.
    const devicesRow = (await view.findByText('Devices')).parent?.parent;
    expect(devicesRow).toBeTruthy();
    expect(within(devicesRow as never).getByText('1')).toBeTruthy();
    expect(view.queryByText('0')).toBeNull();
  });

  it('renames a room in place, with the name it was given', async () => {
    const calls = stubApi();
    const view = await renderPage();

    await fireEvent.press(await view.findByText('Kitchen', {}, { timeout: 5000 }));
    const input = await view.findByPlaceholderText('Room name', {}, { timeout: 5000 });
    // `fireEvent` returns a promise in this version of RNTL: without the await the
    // state update is not flushed and the assertion blames the wrong line.
    await fireEvent.changeText(input, 'Pantry');
    expect(view.getByPlaceholderText('Room name').props.value).toBe('Pantry');
    await fireEvent.press(view.getByText('Save'));

    await waitFor(() => {
      const patch = calls.find((c) => c.method === 'PATCH' && c.url.includes('/rooms/room-1'));
      // The failure message is the whole point: it says what was sent instead.
      expect({ sent: patch?.body, calls: calls.map((c) => `${c.method} ${c.url}`) }).toEqual({
        sent: JSON.stringify({ name: 'Pantry' }),
        calls: expect.any(Array),
      });
    });
  });

  it('asks before deleting a room, because devices fall out of it', async () => {
    const calls = stubApi();
    const view = await renderPage();
    await fireEvent.press(await view.findByLabelText('Delete Kitchen', {}, { timeout: 5000 }));

    // An in-app sheet, not `Alert.alert`: react-native-web implements the native alert as a no-op, so
    // this confirmation used to be invisible on web and the room was never deleted.
    expect(await view.findByText('Delete Kitchen?')).toBeTruthy();
    expect(calls.find((c) => c.method === 'DELETE')).toBeUndefined();

    await fireEvent.press(view.getByTestId('confirm-accept'));

    await waitFor(() => {
      expect(calls.find((c) => c.method === 'DELETE' && c.url.includes('/rooms/room-1'))).toBeTruthy();
    });
  });

  it('declares presence, because automations ask "is anyone home"', async () => {
    const calls = stubApi();
    const view = await renderPage();

    const toggle = await view.findByLabelText('At home', {}, { timeout: 5000 });
    await fireEvent(toggle, 'valueChange', false);

    await waitFor(() => {
      const post = calls.find((c) => c.method === 'POST' && c.url.includes('/presence'));
      expect(post).toBeTruthy();
      expect(JSON.parse(post!.body ?? '{}')).toEqual({ present: false });
    });
  });

  it('signs out', async () => {
    stubApi();
    const view = await renderPage();
    await fireEvent.press(await view.findByText('Sign out', {}, { timeout: 5000 }));

    await waitFor(() => {
      expect(useSession.getState().access).toBeNull();
    });
  });
});
