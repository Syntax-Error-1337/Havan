import { render } from '@testing-library/react-native';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { DashboardPage } from './DashboardPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';

/** Reply by route, so one stub serves the four requests the dashboard makes. */
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
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
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
      if (url.includes('/rooms')) {
        return [{ id: 'room-1', home_id: 'home-1', name: 'Living room', sort_order: 0 }];
      }
      return [
        { id: 'd1', name: 'Lamp', room_id: 'room-1', serial: 'S1', product_key: 'k', category: 'light', online: true, points: [] },
        { id: 'd2', name: 'Plug', room_id: null, serial: 'S2', product_key: 'k', category: 'plug', online: true, points: [] },
      ];
    });

    const view = await renderDashboard();

    // The regression this suite exists for: a response that is a bare array must
    // reach the list. The old `{data}` read left this screen empty forever.
    // `findByText` retries inside act(); `waitFor` + `getByText` raced the query.
    expect(await view.findByText('Lamp')).toBeTruthy();
    expect(view.getByText('Plug')).toBeTruthy();
    expect(view.getByText('Living room')).toBeTruthy();
    expect(view.getByText('Unassigned')).toBeTruthy();
  });

  it('says so plainly when there are no devices', async () => {
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
            device_count: 0,
            created_at: '2026-01-01T00:00:00Z',
          },
        ];
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
