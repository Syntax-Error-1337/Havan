import { render, fireEvent, waitFor } from '@testing-library/react-native';
import { QueryClientProvider } from '@tanstack/react-query';
import { testQueryClient } from '../../test/query';

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
    // `/homes` decides whether the caller may rename, so it answers in every test: a stub that returns
    // a device here is a test saying "there are no homes", which is never what it meant.
    const body = url.endsWith('/homes') ? [homeAsAdmin] : handler(url, init);
    return { ok: true, status: 200, text: async () => JSON.stringify(body) } as Response;
  }) as unknown as typeof fetch;
  return calls;
}

const homeAsAdmin = {
  id: 'home-1',
  owner_id: 'u1',
  name: 'Flat',
  timezone: 'UTC',
  role: 'admin',
  member_count: 2,
  device_count: 1,
  created_at: '2026-01-01T00:00:00Z',
};

async function renderPage() {
  const client = testQueryClient();
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

  it('renames the device in place, and sends only the name', async () => {
    const calls = stubApi((url) => {
      if (url.includes('/rooms')) return [];
      if (url.endsWith('/homes')) return [homeAsAdmin];
      return device;
    });
    const view = await renderPage();
    await view.findByText('Desk lamp');

    // The server has accepted this since the first PATCH; the app simply never offered it, so every
    // home was full of product names.
    fireEvent.press(view.getByTestId('rename-device'));
    // React 19 schedules the re-render; the input is not there on the very next tick.
    await waitFor(() => expect(view.getByLabelText('Device name')).toBeTruthy());
    fireEvent.changeText(view.getByLabelText('Device name'), 'Bedside lamp');
    // The draft has to be committed before Save is pressed: `save` closes over the last render's value,
    // and pressing in the same tick would compare against the old name and send nothing.
    await waitFor(() => expect(view.getByLabelText('Device name').props.value).toBe('Bedside lamp'));
    fireEvent.press(view.getByTestId('rename-save'));

    await waitFor(() =>
      expect(calls.some((c) => c.method === 'PATCH' && c.url.includes('/devices/device-1'))).toBe(
        true,
      ),
    );
    const patchCall = calls.find((c) => c.method === 'PATCH')!;
    expect(JSON.parse(patchCall.body ?? '{}')).toEqual({ name: 'Bedside lamp' });
  });

  it('treats an emptied name as a cancel rather than a request', async () => {
    const calls = stubApi((url) => {
      if (url.includes('/rooms')) return [];
      if (url.endsWith('/homes')) return [homeAsAdmin];
      return device;
    });
    const view = await renderPage();
    await view.findByText('Desk lamp');

    fireEvent.press(view.getByTestId('rename-device'));
    await waitFor(() => expect(view.getByLabelText('Device name')).toBeTruthy());
    fireEvent.changeText(view.getByLabelText('Device name'), '   ');
    await waitFor(() => expect(view.getByLabelText('Device name').props.value).toBe('   '));
    fireEvent.press(view.getByTestId('rename-save'));

    expect(calls.some((c) => c.method === 'PATCH')).toBe(false);
    expect(await view.findByText('Desk lamp')).toBeTruthy();
  });

  it('does not offer a rename a member would be refused', async () => {
    // The server says "only an admin can rename a device". A control that exists only to be refused is
    // worse than no control: the member types a name, presses Save, and learns nothing except "no".
    const calls = stubApi((url) => (url.includes('/rooms') ? [] : device));
    globalThis.fetch = jest.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      calls.push({ url, method: init?.method ?? 'GET', body: init?.body as string });
      const body = url.endsWith('/homes')
        ? [{ ...homeAsAdmin, role: 'member' }]
        : url.includes('/rooms')
          ? []
          : device;
      return { ok: true, status: 200, text: async () => JSON.stringify(body) } as Response;
    }) as unknown as typeof fetch;

    const view = await renderPage();
    await view.findByText('Desk lamp');

    expect(view.queryByTestId('rename-device')).toBeNull();
    expect(view.getByText('Desk lamp')).toBeTruthy();
  });

  it('removes a device only after asking, and says what removal means', async () => {
    const calls = stubApi((url) => (url.includes('/rooms') ? [] : device));

    const view = await renderPage();
    await view.findByText('Desk lamp');

    fireEvent.press(view.getByTestId('remove-device'));

    // Nothing is sent until the person confirms: this is the one action here that cannot be undone by
    // pressing something else.
    expect(calls.some((c) => c.method === 'DELETE')).toBe(false);
    expect(await view.findByText('Remove Desk lamp?')).toBeTruthy();
    expect(view.getByText(/pair it again/)).toBeTruthy();

    fireEvent.press(view.getByTestId('confirm-accept'));

    await waitFor(() =>
      expect(
        calls.some((c) => c.method === 'DELETE' && c.url.includes('/devices/device-1')),
      ).toBe(true),
    );
  });

  it('does not offer removal to someone it would refuse', async () => {
    globalThis.fetch = jest.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.endsWith('/homes')
        ? [{ ...homeAsAdmin, role: 'member' }]
        : url.includes('/rooms')
          ? []
          : device;
      return { ok: true, status: 200, text: async () => JSON.stringify(body) } as Response;
    }) as unknown as typeof fetch;

    const view = await renderPage();
    await view.findByText('Desk lamp');

    expect(view.queryByTestId('remove-device')).toBeNull();
  });

  it('sends a command when the switch is flipped', async () => {
    const calls = stubApi((url) => {
      if (url.includes('/rooms')) return [];
      if (url.endsWith('/homes')) return [homeAsAdmin];
      return device;
    });

    const view = await renderPage();
    await view.findByText('Desk lamp');

    // The control is a press, not a native Switch's onValueChange: it is one component for every
    // platform, and on web the native switch does not exist.
    fireEvent.press(view.getAllByRole('switch')[0]!);

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

    // The chart says what the window held, in the customer's terms. It used to add "21 readings · 2 bars" —
    // the platform's own plumbing: how many samples it stored and how many columns it drew.
    expect(await view.findByText(/over the last day/)).toBeTruthy();
    expect(view.queryByText(/readings ·/)).toBeNull();
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

    // Not "no readings yet" in the abstract: the last value is the one fact available, and the window
    // makes "nothing here" a statement about a period rather than about the device.
    expect(await view.findByText(/No readings in the last day — the latest is/)).toBeTruthy();
  });

  it('refuses to control an offline device', async () => {
    const calls = stubApi((url) =>
      url.includes('/rooms') ? [] : { ...device, online: false },
    );

    const view = await renderPage();
    await view.findByText('Desk lamp');

    // An offline device cannot act on a command, so the control says so rather than swallowing the
    // tap: a switch that looks live and does nothing is the worse of the two lies.
    const control = view.getAllByRole('switch')[0]!;
    expect(control.props.accessibilityState?.disabled).toBe(true);
    fireEvent.press(control);
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
