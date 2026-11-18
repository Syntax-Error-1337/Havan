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

describe('a brand-new account', () => {
  it('creates the first home in the customer\'s words and their own timezone', async () => {
    // It used to post `{ name: 'My home', timezone: 'UTC' }` without asking: a name nobody chose, and a
    // zone that put every schedule in the wrong hour for everybody outside London.
    const posts: { url: string; body: string | undefined }[] = [];
    // The homes list is empty until the POST happens — otherwise `useCurrentHome` adopts the home this
    // test is about to create and the first-run screen never shows.
    let created = false;
    globalThis.fetch = jest.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (init?.method === 'POST') {
        posts.push({ url, body: init.body as string | undefined });
        created = true;
        return {
          ok: true,
          status: 201,
          text: async () => JSON.stringify({ id: 'home-new', name: 'Flat 3', timezone: 'Europe/Berlin', role: 'owner' }),
        } as Response;
      }
      // After the create, the next render asks for the homes list: answer with the new home, or the
      // screen looks for a home that its own POST just invented.
      return {
        ok: true,
        status: 200,
        text: async () =>
          JSON.stringify(
            url.endsWith('/homes') && !created
              ? []
              : url.endsWith('/homes')
              ? [{ id: 'home-new', name: 'Flat 3', timezone: 'Europe/Berlin', role: 'owner', member_count: 1 }]
              : url.includes('/dashboard')
                ? { home: { id: 'home-new', name: 'Flat 3' }, rooms: [], devices: [], scenes: [], unread_count: 0 }
                : [],
          ),
      } as Response;
    }) as unknown as typeof fetch;

    const view = await renderDashboard();
    // By testID: the input sits inside a labelled wrapper, and `findByLabelText` may hand back the
    // wrapper, where `changeText` does nothing at all.
    const nameField = await view.findByTestId('first-home-name');
    // Prefilled with the name it will create: as a placeholder, "Create home" quietly made a home called
    // "My home" that the customer had never seen, and the screen's outcome disagreed with its own field.
    expect(nameField.props.value).toBe('My home');
    fireEvent.changeText(nameField, 'Flat 3');
    await waitFor(() => expect(nameField.props.value).toBe('Flat 3'));
    fireEvent.press(view.getByText('Create home'));

    await waitFor(() => expect(posts.some((p) => p.url.includes('/homes'))).toBe(true));
    const body = JSON.parse(posts.find((p) => p.url.endsWith('/homes'))!.body ?? '{}');
    expect(body.name).toBe('Flat 3');
    // A real IANA zone from the device, not the literal 'UTC' every home used to be created in.
    expect(typeof body.timezone).toBe('string');
    expect(body.timezone.length).toBeGreaterThan(0);
  });

  it('offers a way out for the wrong account', async () => {
    // Signed in as somebody else, the first-run screen was a dead end: no home, no exit.
    const signOut = jest.fn(async () => undefined);
    useSession.setState({ access: 'token', homeId: null, refresh: null, signOut });
    stubApi((url) => (url.endsWith('/homes') ? [] : null));

    const view = await renderDashboard();
    fireEvent.press(await view.findByText('Use a different account'));

    await waitFor(() => expect(signOut).toHaveBeenCalled());
  });
});

describe('an empty home', () => {
  it('tells a new customer what to do, in the order people actually do it', async () => {
    stubApi((url) => {
      if (url.endsWith('/homes')) {
        return [{ id: 'home-1', name: 'Flat 3', role: 'owner', member_count: 1 }];
      }
      if (url.includes('/dashboard')) {
        return {
          home: { id: 'home-1', name: 'Flat 3' },
          rooms: [],
          devices: [],
          scenes: [],
          unread_count: 0,
          presence: { total: 0, online: 0, silent: 0 },
        };
      }
      return [];
    });
    useSession.setState({ access: 'token', homeId: 'home-1', refresh: null });

    const view = await renderDashboard();

    expect(await view.findByTestId('first-run-home')).toBeTruthy();
    // Pairing first: nobody unboxes a bulb and then wants to name an empty room.
    expect(view.getByText('Power the device on')).toBeTruthy();
    expect(view.getByText('Pair it from your phone')).toBeTruthy();
    expect(view.getByText('Give it a room')).toBeTruthy();
    // A search box on a home with nothing in it is a control that can only fail.
    expect(view.queryByTestId('device-search')).toBeNull();
    // And no "Try again" — nothing failed.
    expect(view.queryByText('Try again')).toBeNull();
  });
});

describe('dashboard', () => {
  it('says the home could not be loaded instead of pretending it is empty', async () => {
    // Measured with the dashboard request blocked: a home with six devices told the customer "No devices
    // yet - Tap Add and we will look for devices nearby", which invites re-pairing hardware that is
    // already on the wall. A failed request is not an empty house.
    globalThis.fetch = jest.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes('/dashboard')) {
        return { ok: false, status: 500, text: async () => '{"error":"boom"}' } as Response;
      }
      return {
        ok: true,
        status: 200,
        text: async () =>
          JSON.stringify(url.endsWith('/homes') ? [{ id: 'home-1', name: 'Workshop', role: 'owner', member_count: 1 }] : []),
      } as Response;
    }) as unknown as typeof fetch;

    const view = await renderDashboard();

    expect(await view.findByTestId('home-unreachable')).toBeTruthy();
    expect(view.queryByText('No devices yet')).toBeNull();
    // The counts are not known either, so the header does not print zeroes.
    expect(view.getByText('— devices · — rooms')).toBeTruthy();
  });

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

    // The empty home is now a three-step list rather than one grey line, and its heading names the home.
    expect(await view.findByText('Flat is empty')).toBeTruthy();
    expect(view.getByText('Pair a device')).toBeTruthy();
  });

  // Three creates used to live in three places: the centre button went straight to a device scan, "New
  // room" was a chip among the filters, and "Add a scene" was a dashed card in the strip. One button now
  // asks which, and each answer lands somewhere sensible.
  it('asks which kind of thing to add instead of assuming a device', async () => {
    stubApi((url) => {
      if (url.endsWith('/homes')) {
        return [
          {
            id: 'home-1',
            owner_id: 'u1',
            name: 'Flat',
            timezone: 'UTC',
            role: 'owner',
            member_count: 1,
            device_count: 1,
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
    fireEvent.press(await view.findByLabelText('Add'));

    // The sheet names the home, so the person knows where it lands.
    expect(await view.findByText('ADD TO FLAT')).toBeTruthy();
    expect(view.getByText('Add a device')).toBeTruthy();
    expect(view.getByText('Add a room')).toBeTruthy();
    expect(view.getByText('Add a scene')).toBeTruthy();

    // Choosing a room reveals the form in the list it will join, rather than navigating away from it.
    fireEvent.press(view.getByTestId('action-room'));
    expect(await view.findByPlaceholderText('Room name')).toBeTruthy();
    expect(view.queryByText('Add a device')).toBeNull();
  });

  // The endpoint answers 403 to a member. Offering the action anyway produced a sheet that did nothing:
  // the rule is known from the homes list, so the option is drawn only when it will work.
  it('does not offer to add a room to a member who could not', async () => {
    stubApi((url) => {
      if (url.endsWith('/homes')) {
        return [
          {
            id: 'home-1',
            owner_id: 'u1',
            name: 'Flat',
            timezone: 'UTC',
            role: 'member',
            member_count: 2,
            device_count: 1,
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
    fireEvent.press(await view.findByLabelText('Add'));

    expect(await view.findByText('Add a device')).toBeTruthy();
    expect(view.queryByText('Add a room')).toBeNull();
    expect(view.getByText('Add a scene')).toBeTruthy();
  });

  it('offers to create a home when the account has none', async () => {
    stubApi(() => []);

    const view = await renderDashboard();

    expect(await view.findByText('Name your home')).toBeTruthy();
  });
});
