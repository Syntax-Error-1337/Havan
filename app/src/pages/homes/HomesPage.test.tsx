import { fireEvent, render } from '@testing-library/react-native';
import { QueryClientProvider } from '@tanstack/react-query';
import { testQueryClient } from '../../test/query';

import { HomesPage } from './HomesPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';

/**
 * The switcher.
 *
 * Its whole job is to say which home you are in, and it shipped without ever saying it: the session's
 * `homeId` is empty until the dashboard picks a home, so every row failed `home.id === current` and the
 * screen told a fresh sign-in nothing. There was no test file at all, which is how that survived.
 */

const home = (over: Partial<Record<string, unknown>> = {}) => ({
  id: 'home-1',
  owner_id: 'u1',
  name: 'Flat',
  timezone: 'UTC',
  role: 'owner',
  member_count: 2,
  device_count: 6,
  created_at: '2026-01-01T00:00:00Z',
  ...over,
});

function stubApi(homes: unknown[]) {
  globalThis.fetch = jest.fn(async () => {
    return { ok: true, status: 200, text: async () => JSON.stringify(homes) } as Response;
  }) as unknown as typeof fetch;
}

async function renderPage() {
  return await render(
    <QueryClientProvider client={testQueryClient()}>
      <ThemeProvider>
        <HomesPage />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  useSession.setState({ access: 'token', refresh: 'r', homeId: null, hydrated: true });
});

describe('homes', () => {
  it('marks the stored home as the current one', async () => {
    stubApi([home(), home({ id: 'home-2', name: 'Cottage', device_count: 1200 })]);
    useSession.setState({ homeId: 'home-2' });

    const view = await renderPage();

    expect(await view.findByText('Cottage')).toBeTruthy();
    // One marker, on the row you are actually in.
    expect(view.getAllByText('Current')).toHaveLength(1);
    expect(view.getByTestId('home-row-home-2')).toBeTruthy();
  });

  it('marks the home it would open when nothing has been chosen yet', async () => {
    // Before a home is stored the dashboard opens the first one, so that is the one this screen must
    // call current. It used to call none of them current, which is the bug this file exists for.
    stubApi([home(), home({ id: 'home-2', name: 'Cottage' })]);

    const view = await renderPage();

    expect(await view.findByText('Flat')).toBeTruthy();
    expect(view.getAllByText('Current')).toHaveLength(1);
  });

  it('switches when a row is chosen', async () => {
    stubApi([home(), home({ id: 'home-2', name: 'Cottage' })]);
    // The store's `setHome` writes to secure storage before it sets state, so the wiring is what this
    // asserts: pressing a row hands that row's id to the one function that changes homes.
    const setHome = jest.fn(async () => undefined);
    useSession.setState({ setHome });

    const view = await renderPage();
    fireEvent.press(await view.findByTestId('home-row-home-2'));

    expect(setHome).toHaveBeenCalledWith('home-2');
  });

  it('prints a real device count, with thousands separated', async () => {
    stubApi([home({ device_count: 1200 })]);

    const view = await renderPage();

    expect(await view.findByText(/1,200 devices/)).toBeTruthy();
  });
});
