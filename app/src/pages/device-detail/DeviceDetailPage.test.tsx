import { render, fireEvent, waitFor } from '@testing-library/react-native';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { DeviceDetailPage } from './DeviceDetailPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';

const history = {
  device_id: 'device-1',
  code: 'brightness.level',
  from: '2026-01-01T00:00:00Z',
  to: '2026-01-02T00:00:00Z',
  bucket_secs: 1440,
  buckets: [
    { t: '2026-01-01T00:00:00Z', v: 40, n: 12 },
    { t: '2026-01-01T01:00:00Z', v: 80, n: 9 },
  ],
};

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

  it('charts a day by default, and asks for another range when told', async () => {
    const calls = stubApi((url) => {
      if (url.includes('/history')) return history;
      if (url.includes('/rooms')) return [];
      return device;
    });

    const view = await renderPage();
    await view.findByText('Desk lamp');

    // The chart reports what it drew, so an empty plot is never ambiguous.
    // 12 + 9 samples across two buckets — the count is the point.
    expect(await view.findByText(/21 readings · 2 bars/)).toBeTruthy();
    expect(calls.some((c) => c.url.includes('range=1d'))).toBe(true);

    await fireEvent.press(view.getByRole('button', { name: '7d' }));

    await waitFor(() => expect(calls.some((c) => c.url.includes('range=7d'))).toBe(true));
  });

  it('says so instead of drawing an empty chart', async () => {
    stubApi((url) => {
      if (url.includes('/history')) return { ...history, buckets: [] };
      if (url.includes('/rooms')) return [];
      return device;
    });

    const view = await renderPage();
    await view.findByText('Desk lamp');

    expect(
      await view.findByText(/No readings yet\. History starts after the device reports/),
    ).toBeTruthy();
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

  it('does not offer a chart for a point the platform does not store', async () => {
    // `history: off` is a policy, not a fault: drawing an empty chart would say
    // "no readings yet" forever and read as a bug.
    stubApi((url) => {
      if (url.includes('/history')) return { ...history, buckets: [] };
      if (url.includes('/rooms')) return [];
      return { ...device, points: device.points.map((pt) => ({ ...pt, history: 'off' })) };
    });
    const view = await renderPage();

    expect(await view.findByText('This device does not keep history.')).toBeTruthy();
    // No range chips either: there is nothing to range over.
    expect(view.queryByText('7d')).toBeNull();
  });
});
