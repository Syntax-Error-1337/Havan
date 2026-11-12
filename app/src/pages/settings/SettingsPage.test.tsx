import { render, fireEvent, waitFor } from '@testing-library/react-native';
import { QueryClientProvider } from '@tanstack/react-query';
import { Alert } from 'react-native';

import { testQueryClient } from '../../test/query';
import { SettingsPage } from './SettingsPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';

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
      if (url.endsWith('/dashboard'))
        return { home, rooms: [room], devices: [], scenes: [], unread_count: 0 };
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

describe('settings', () => {
  it('shows who is signed in', async () => {
    stubApi();
    const view = await renderPage();
    expect(await view.findByText('demo@haven.dev', {}, { timeout: 5000 })).toBeTruthy();
    expect(await view.findByText('Demo flat', {}, { timeout: 5000 })).toBeTruthy();
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
    const alert = jest.spyOn(Alert, 'alert').mockImplementation(() => {});

    const view = await renderPage();
    await fireEvent.press(await view.findByLabelText('Delete Kitchen', {}, { timeout: 5000 }));

    expect(alert).toHaveBeenCalled();
    expect(calls.find((c) => c.method === 'DELETE')).toBeUndefined();

    // The destructive button is the one that actually deletes.
    const buttons = alert.mock.calls[0]![2] as { style?: string; onPress?: () => void }[];
    buttons.find((b) => b.style === 'destructive')!.onPress!();

    await waitFor(() => {
      expect(calls.find((c) => c.method === 'DELETE' && c.url.includes('/rooms/room-1'))).toBeTruthy();
    });
    alert.mockRestore();
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
