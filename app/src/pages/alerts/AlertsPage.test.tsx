import { render, fireEvent, waitFor } from '@testing-library/react-native';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { AlertsPage } from './AlertsPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';

const alert = (over: Partial<Record<string, unknown>> = {}) => ({
  id: 'n1',
  kind: 'automation',
  title: 'Lamp on',
  body: 'The desk lamp is on again',
  data: {},
  created_at: new Date(Date.now() - 5 * 60_000).toISOString(),
  read_at: null,
  ...over,
});

function stubApi(handler: (url: string, init?: RequestInit) => unknown) {
  const calls: { url: string; method: string }[] = [];
  globalThis.fetch = jest.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, method: init?.method ?? 'GET' });
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
        <AlertsPage />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  useSession.setState({ access: 'token', homeId: 'home-1', refresh: null });
});

describe('alerts', () => {
  it('shows an unread alert with how long ago it fired', async () => {
    stubApi(() => [alert()]);
    const view = await renderPage();

    expect(await view.findByText('Lamp on')).toBeTruthy();
    expect(view.getByText('The desk lamp is on again')).toBeTruthy();
    expect(view.getByText('5 min ago')).toBeTruthy();
    expect(view.getByText('1 unread')).toBeTruthy();
  });

  it('marks an alert read when it is tapped, and not before', async () => {
    const calls = stubApi((url) => (url.includes('/read') ? { changed: true } : [alert()]));
    const view = await renderPage();

    await view.findByText('Lamp on');
    expect(calls.some((c) => c.method === 'POST')).toBe(false);

    await fireEvent.press(view.getByText('Lamp on'));

    await waitFor(() => expect(calls.some((c) => c.url.includes('/notifications/n1/read'))).toBe(true));
  });

  it('does not offer to mark a read alert again', async () => {
    stubApi(() => [alert({ read_at: '2026-01-01T00:00:00Z' })]);
    const view = await renderPage();

    await view.findByText('Lamp on');
    // Read alerts are inert: their press handler is gone, and the header says so.
    expect(view.getByText('Nothing new')).toBeTruthy();
    expect(view.queryByRole('button', { name: 'Mark all read' })).toBeNull();
  });

  it('says what an empty inbox means instead of showing blank space', async () => {
    stubApi(() => []);
    const view = await renderPage();

    expect(await view.findByText('Nothing yet')).toBeTruthy();
    expect(view.getByText(/Automations write here/)).toBeTruthy();
  });

  it('marks everything read in one request', async () => {
    const calls = stubApi((url) => (url.includes('read-all') ? { cleared: 3 } : [alert()]));
    const view = await renderPage();

    await view.findByText('Lamp on');
    await fireEvent.press(view.getByText('Mark all read'));

    await waitFor(() => expect(calls.some((c) => c.url.includes('/notifications/read-all'))).toBe(true));
  });
});
