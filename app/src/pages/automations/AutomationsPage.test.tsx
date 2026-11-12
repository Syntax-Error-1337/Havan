import { render, fireEvent, waitFor } from '@testing-library/react-native';
import { QueryClientProvider } from '@tanstack/react-query';
import { testQueryClient } from '../../test/query';

import { AutomationsPage } from './AutomationsPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';

const home = {
  id: 'home-1',
  owner_id: 'u1',
  name: 'Flat',
  timezone: 'UTC',
  role: 'owner',
  member_count: 1,
  device_count: 1,
  created_at: '2026-01-01T00:00:00Z',
};

const rule = {
  id: 'rule-1',
  home_id: 'home-1',
  name: 'Bathroom fan',
  enabled: true,
  definition: {
    trigger: { type: 'schedule', every_secs: 300 },
    conditions: [],
    actions: [{ type: 'set_device', device_id: 'd1', code: 'switch.power', value: true }],
  },
  next_run_at: null,
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
  const client = testQueryClient();
  return await render(
    <QueryClientProvider client={client}>
      <ThemeProvider>
        <AutomationsPage />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  useSession.setState({ access: 'token', homeId: 'home-1', refresh: null });
});

describe('automations', () => {
  it('lists a rule in words a person can read', async () => {
    stubApi((url) => (url.endsWith('/automations') ? [rule] : [home]));

    const view = await renderPage();

    expect(await view.findByText('Bathroom fan')).toBeTruthy();
    // describe() turns the stored JSON into a sentence: the list is useless if
    // it shows a definition object.
    expect(view.getByText('Every 5 min → set switch.power')).toBeTruthy();
  });

  it('flips the switch through the API, not just on screen', async () => {
    const calls = stubApi((url) =>
      url.endsWith('/automations') ? [rule] : [{ ...rule, enabled: false }],
    );

    const view = await renderPage();
    await view.findByText('Bathroom fan');

    fireEvent(view.getByRole('switch'), 'onValueChange', false);

    // The mutation is async: assert on the request it made, not on the render.
    await waitFor(() => expect(calls.some((c) => c.method === 'PATCH')).toBe(true));

    const patch = calls.find((c) => c.method === 'PATCH');
    expect(patch?.url).toContain('/automations/rule-1');
    expect(JSON.parse(patch?.body ?? '{}')).toEqual({ enabled: false });
  });

  it('shows the run log when history is opened', async () => {
    stubApi((url) => {
      if (url.includes('/runs')) {
        return [
          {
            id: 'run-1',
            trigger_kind: 'schedule',
            status: 'completed',
            started_at: '2026-01-01T10:00:00Z',
            log: {
              steps: [
                { kind: 'trigger', detail: 'every 5 minutes' },
                { kind: 'action', index: 0, detail: 'set switch.power = true' },
              ],
            },
          },
        ];
      }
      return url.endsWith('/automations') ? [rule] : [home];
    });

    const view = await renderPage();
    await view.findByText('Bathroom fan');

    await fireEvent.press(view.getByText('History'));

    expect(await view.findByText(/every 5 minutes/)).toBeTruthy();
    expect(view.getByText(/set switch\.power = true/)).toBeTruthy();
  });

  it('says a rule has never run instead of showing an empty box', async () => {
    stubApi((url) => (url.includes('/runs') ? [] : url.endsWith('/automations') ? [rule] : [home]));

    const view = await renderPage();
    await view.findByText('Bathroom fan');
    await fireEvent.press(view.getByText('History'));

    expect(await view.findByText(/Never run/)).toBeTruthy();
  });
});
