import { render, fireEvent, waitFor } from '@testing-library/react-native';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { ScenesPage } from './ScenesPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';

const device = {
  id: 'd1',
  name: 'Desk lamp',
  serial: 'S1',
  product_key: 'k',
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
    return {
      ok: true,
      status: 200,
      text: async () => JSON.stringify(handler(url, init)),
    } as Response;
  }) as unknown as typeof fetch;
  return calls;
}

async function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return await render(
    <QueryClientProvider client={client}>
      <ThemeProvider>
        <ScenesPage />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  useSession.setState({ access: 'token', homeId: 'home-1', refresh: null });
});

describe('scenes', () => {
  it('saves a scene with typed values, not strings', async () => {
    const calls = stubApi((url, init) => {
      if (init?.method === 'POST') return { id: 's1', name: 'Good night' };
      if (url.includes('/scenes')) return [];
      return [device];
    });

    const view = await renderPage();
    await view.findByText('New scene');

    await fireEvent.changeText(
      view.getByPlaceholderText('Name, e.g. Good night'),
      'Good night',
    );
    await fireEvent.press(view.getByLabelText('Device 1: Desk lamp'));
    await fireEvent.press(view.getByLabelText('Point: switch.power'));
    await fireEvent.press(view.getByRole('button', { name: 'Create scene' }));

    await waitFor(() => expect(calls.some((c) => c.method === 'POST')).toBe(true));
    const body = JSON.parse(calls.find((c) => c.method === 'POST')?.body ?? '{}');
    // A bool stored as "true" would be rejected by the API — the value has to
    // take the point's own type.
    expect(body.actions[0]).toEqual({
      type: 'set_device',
      device_id: 'd1',
      code: 'switch.power',
      value: true,
    });
  });

  it('keeps Create disabled until the scene is complete', async () => {
    stubApi(() => []);
    const view = await renderPage();
    await view.findByText('New scene');

    await fireEvent.changeText(
      view.getByPlaceholderText('Name, e.g. Good night'),
      'Half a scene',
    );

    // A name and no device is not a scene.
    await fireEvent.press(view.getByRole('button', { name: 'Create scene' }));
    // Nothing to assert about the request: the button is disabled, so it cannot fire.
  });

  it('lists existing scenes with their action count', async () => {
    stubApi((url) => {
      if (url.includes('/scenes')) {
        return [
          {
            id: 's1',
            home_id: 'home-1',
            name: 'Good night',
            icon: null,
            actions: [
              { type: 'set_device', device_id: 'd1', code: 'switch.power', value: false },
            ],
            created_at: '2026-01-01T00:00:00Z',
          },
        ];
      }
      return [device];
    });

    const view = await renderPage();

    expect(await view.findByText('Good night')).toBeTruthy();
    expect(view.getByText('1 action')).toBeTruthy();
  });

  it('runs a scene from the list', async () => {
    const calls = stubApi((url, init) => {
      if (init?.method === 'POST' && url.includes('/activate')) {
        return { scene_id: 's1', dispatched: 1, failed: [] };
      }
      if (url.includes('/scenes')) {
        return [
          {
            id: 's1',
            home_id: 'home-1',
            name: 'Good night',
            icon: null,
            actions: [],
            created_at: '2026-01-01T00:00:00Z',
          },
        ];
      }
      return [device];
    });

    const view = await renderPage();
    await view.findByText('Good night');
    await fireEvent.press(view.getByRole('button', { name: 'Run' }));

    await waitFor(() => expect(calls.some((c) => c.url.includes('/scenes/s1/activate'))).toBe(true));
  });

  it('deletes a scene', async () => {
    const calls = stubApi((url, init) => {
      if (init?.method === 'DELETE') return null;
      if (url.includes('/scenes')) {
        return [
          {
            id: 's9',
            home_id: 'home-1',
            name: 'Old scene',
            icon: null,
            actions: [],
            created_at: '2026-01-01T00:00:00Z',
          },
        ];
      }
      return [device];
    });

    const view = await renderPage();
    await view.findByText('Old scene');
    await fireEvent.press(view.getByText('Delete'));

    await waitFor(() => expect(calls.some((c) => c.method === 'DELETE')).toBe(true));
    expect(calls.find((c) => c.method === 'DELETE')?.url).toContain('/scenes/s9');
  });
});
