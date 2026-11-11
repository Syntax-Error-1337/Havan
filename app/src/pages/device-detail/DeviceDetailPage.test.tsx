import { render, fireEvent, waitFor } from '@testing-library/react-native';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { DeviceDetailPage } from './DeviceDetailPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';

const device = {
  id: 'device-1',
  home_id: 'home-1',
  name: 'Desk lamp',
  serial: 'ACME-0001',
  room_id: null as string | null,
  room_name: null as string | null,
  product_key: 'hv_acme_bulb1',
  category: 'light',
  online: true,
  points: [
    {
      id: 'p1',
      code: 'switch.power',
      capability: 'switch',
      slot: 'power',
      kind: 'bool',
      writable: true,
      value: false,
    },
    {
      id: 'p2',
      code: 'brightness.level',
      capability: 'brightness',
      slot: 'level',
      kind: 'int',
      writable: true,
      min: 0,
      max: 100,
      value: 40,
    },
  ],
};

function stubApi(handler: (url: string, init?: RequestInit) => unknown) {
  const calls: { url: string; method: string; body?: string }[] = [];
  globalThis.fetch = jest.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, method: init?.method ?? 'GET', body: init?.body as string });
    return { ok: true, status: 200, text: async () => JSON.stringify(handler(url, init)) } as Response;
  }) as unknown as typeof fetch;
  return calls;
}

async function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  return await render(
    <QueryClientProvider client={client}>
      <ThemeProvider>
        <DeviceDetailPage />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  useSession.setState({ access: 'token', homeId: 'home-1', refresh: null });
});

describe('device detail', () => {
  it('assigns a room with an explicit null-able field', async () => {
    const calls = stubApi((url) => {
      if (url.includes('/rooms')) {
        return [{ id: 'room-1', home_id: 'home-1', name: 'Study', sort_order: 0 }];
      }
      return device;
    });

    const view = await renderPage();
    await view.findByText('Desk lamp');

    await fireEvent.press(view.getByText('Study'));

    await waitFor(() => expect(calls.some((c) => c.method === 'PATCH')).toBe(true));
    const patch = calls.find((c) => c.method === 'PATCH');
    expect(patch?.url).toContain('/devices/device-1');
    // 'absent' and 'null' mean different things to the API; this must be null.
    expect(JSON.parse(patch?.body ?? '{}')).toEqual({ room_id: 'room-1' });
  });

  it('sends a command when the switch is flipped', async () => {
    const calls = stubApi((url) => (url.includes('/rooms') ? [] : device));

    const view = await renderPage();
    await view.findByText('Desk lamp');

    fireEvent(view.getByRole('switch'), 'onValueChange', true);

    await waitFor(() => expect(calls.some((c) => c.method === 'POST')).toBe(true));
    const post = calls.find((c) => c.method === 'POST');
    expect(post?.url).toContain('/devices/device-1/commands');
    expect(JSON.parse(post?.body ?? '{}')).toEqual({ code: 'switch.power', value: true });
  });

  it('refuses to control an offline device', async () => {
    const calls = stubApi((url) =>
      url.includes('/rooms') ? [] : { ...device, online: false },
    );

    const view = await renderPage();
    await view.findByText('Desk lamp');

    fireEvent(view.getByRole('switch'), 'onValueChange', true);
    expect(calls.some((c) => c.method === 'POST')).toBe(false);
  });
});
