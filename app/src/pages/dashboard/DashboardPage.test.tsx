import { fireEvent, render, waitFor } from '@testing-library/react-native';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { DashboardPage } from './DashboardPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';

/** Reply by route. The screen asks for one aggregate; `/homes` still decides whether
 *  an account has a home at all. */
function stubApi(handler: (url: string) => unknown) {
  globalThis.fetch = jest.fn(async (input: RequestInfo | URL) => {
    const url = String(input);
    return {
      ok: true,
      status: 200,
      text: async () => JSON.stringify(handler(url)),
    } as Response;
  }) as unknown as typeof fetch;
}

async function renderDashboard() {
  const client = new QueryClient({
    // No `gcTime: 0`: the screen seeds the devices/scenes caches from the aggregate it just fetched,
    // and an immediate collection can take them between the fetch resolving and the list rendering —
    // which is how this suite flakes to "cannot find Lamp" under parallel load. Nothing here needs
    // eager collection; the client dies with the test.
    defaultOptions: { queries: { retry: false, gcTime: Infinity } },
  });

  return await render(
    <QueryClientProvider client={client}>
      <ThemeProvider>
        <DashboardPage />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  useSession.setState({ access: null, homeId: null, refresh: null });
});

describe('dashboard', () => {
  it('shows devices grouped by room, and the unassigned ones apart', async () => {
    stubApi((url) => {
      if (url.includes('/scenes')) return [];
      if (url.endsWith('/homes')) {
        return [
          {
            id: 'home-1',
            owner_id: 'u1',
            name: 'Flat',
            timezone: 'UTC',
            role: 'owner',
            member_count: 1,
            device_count: 2,
            created_at: '2026-01-01T00:00:00Z',
          },
        ];
      }
      if (url.includes('/dashboard')) {
        // One response for the whole screen, as the server now composes it.
        return {
          home: { id: 'home-1', name: 'Flat', timezone: 'UTC' },
          rooms: [{ id: 'room-1', home_id: 'home-1', name: 'Living room', sort: 0 }],
          devices: [
            { id: 'd1', name: 'Lamp', room_id: 'room-1', serial: 'S1', product_key: 'k', category: 'light', online: true, points: [] },
            { id: 'd2', name: 'Plug', room_id: null, serial: 'S2', product_key: 'k', category: 'plug', online: true, points: [] },
          ],
          scenes: [],
          unread_count: 0,
        };
      }
      return [];
    });

    const view = await renderDashboard();

    // The regression this suite exists for: a response that is a bare array must
    // reach the list. The old `{data}` read left this screen empty forever.
    // `findByText` retries inside act(); `waitFor` + `getByText` raced the query.
    expect(await view.findByText('Lamp')).toBeTruthy();
    // "Plug" is now both the device's name and a category chip, which is the point of the chip row —
    // so the assertion is that the device itself rendered, by the tile's own test id.
    expect(view.getByTestId('device-tile-S2')).toBeTruthy();
    expect(view.getByText('Living room')).toBeTruthy();
    expect(view.getByText('Unassigned')).toBeTruthy();
  });

  it('offers the rest when the home is bigger than one page', async () => {
    stubApi((url) => {
      if (url.includes('/homes/') && url.includes('/dashboard')) {
        return {
          home: { id: 'home-1', name: 'Big flat', timezone: 'UTC' },
          rooms: [{ id: 'room-1', home_id: 'home-1', name: 'Living room', sort: 0 }],
          devices: [
            { id: 'd1', name: 'Lamp', room_id: 'room-1', serial: 'S1', product_key: 'k', category: 'light', online: true, points: [] },
          ],
          scenes: [],
          unread_count: 0,
          // The server's own words: this answer is a page of a larger home.
          devices_total: 431,
          devices_next_cursor: '0:4c616d70:01a1fd00-0000-7000-8000-000000000001',
        };
      }
      return [{ id: 'home-1', name: 'Big flat', timezone: 'UTC', role: 'owner', device_count: 431, member_count: 1, created_at: '', owner_id: 'u' }];
    });

    const view = await renderDashboard();
    await view.findByText('Lamp');

    // Both halves matter: the count is the home's, and the footer says how much is behind it. The count
    // moved into the header pill, and the line under the title says the home is being shown a page at a time.
    expect(view.getByText(/1 of 431 online/)).toBeTruthy();
    expect(view.getByText(/431 devices · showing 1/)).toBeTruthy();
    expect(view.getByText(/Load 430 more/)).toBeTruthy();
    expect(view.getByText(/showing 1 of 431/)).toBeTruthy();
  });

  // The bug this test is for: "more" used to be a growing `device_limit`, and the server clamps a page
  // at 500. On a 1,200-device home the first press asked for 800, was given the same first 500 again,
  // and the cursor never moved — the last 700 devices were unreachable while the footer kept offering
  // them. A cursor is the only thing that advances.
  it('walks past the server’s page cap a cursor at a time', async () => {
    const tile = (n: number) => ({
      id: `d${n}`,
      name: `Device ${n}`,
      room_id: null,
      serial: `S${n}`,
      product_key: 'k',
      category: 'plug',
      online: true,
      points: [],
    });

    stubApi((url) => {
      if (url.includes('/dashboard') && url.includes('device_cursor=')) {
        // Second page: 500 devices, and a cursor for the rest.
        return {
          home: { id: 'home-1', name: 'Plant', timezone: 'UTC' },
          rooms: [],
          devices: Array.from({ length: 500 }, (_, i) => tile(201 + i)),
          scenes: [],
          unread_count: 0,
          devices_total: 1200,
          devices_next_cursor: 'cursor-2',
        };
      }
      if (url.includes('/dashboard')) {
        return {
          home: { id: 'home-1', name: 'Plant', timezone: 'UTC' },
          rooms: [],
          devices: Array.from({ length: 200 }, (_, i) => tile(i + 1)),
          scenes: [],
          unread_count: 0,
          devices_total: 1200,
          devices_next_cursor: 'cursor-1',
        };
      }
      return [{ id: 'home-1', name: 'Plant', timezone: 'UTC', role: 'owner', device_count: 1200, member_count: 1, created_at: '', owner_id: 'u' }];
    });

    const view = await renderDashboard();
    await view.findByText('Device 1');

    // The press says what it will do, not what is left in the home: the server will not give more.
    expect(view.getByText(/Load 500 more/)).toBeTruthy();
    expect(view.getByText(/showing 200 of 1200/)).toBeTruthy();

    fireEvent.press(view.getByTestId('load-all-devices'));

    // 200 from the first page and 500 from the tail, deduped by id, and the footer now offers the rest.
    await waitFor(() => expect(view.getByText(/showing 700 of 1200/)).toBeTruthy());
    expect(view.getByText(/Load 500 more/)).toBeTruthy();
  });

  it('says so plainly when there are no devices', async () => {
    stubApi((url) => {
      if (url.includes('/scenes')) return [];
      if (url.endsWith('/homes')) {
        return [
          {
            id: 'home-1',
            owner_id: 'u1',
            name: 'Flat',
            timezone: 'UTC',
            role: 'owner',
            member_count: 1,
            device_count: 0,
            created_at: '2026-01-01T00:00:00Z',
          },
        ];
      }
      if (url.includes('/dashboard')) {
        return {
          home: { id: 'home-1', name: 'Flat', timezone: 'UTC' },
          rooms: [],
          devices: [],
          scenes: [],
          unread_count: 0,
        };
      }
      return [];
    });

    const view = await renderDashboard();

    expect(await view.findByText('No devices yet')).toBeTruthy();
  });

  it('offers to create a home when the account has none', async () => {
    stubApi(() => []);

    const view = await renderDashboard();

    expect(await view.findByText('Create your first home')).toBeTruthy();
  });
});
