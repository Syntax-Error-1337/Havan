import { render, fireEvent, waitFor } from '@testing-library/react-native';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { SceneStrip } from './SceneStrip';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';

const scene = (over: Partial<Record<string, unknown>> = {}) => ({
  id: 's1',
  home_id: 'home-1',
  name: 'Good night',
  icon: 'moon',
  actions: [
    { type: 'set_device', device_id: 'd1', code: 'switch.power', value: false },
    { type: 'set_device', device_id: 'd1', code: 'brightness.level', value: 5 },
  ],
  created_at: '2026-01-01T00:00:00Z',
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

async function renderStrip() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return await render(
    <QueryClientProvider client={client}>
      <ThemeProvider>
        <SceneStrip homeId="home-1" />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  useSession.setState({ access: 'token', homeId: 'home-1', refresh: null });
});

describe('scene strip', () => {
  it('shows a scene with how many devices it touches', async () => {
    stubApi(() => [scene()]);
    const view = await renderStrip();

    expect(await view.findByText('Good night')).toBeTruthy();
    expect(view.getByText('2 devices')).toBeTruthy();
  });

  it('renders nothing at all when a home has no scenes', async () => {
    stubApi(() => []);
    const view = await renderStrip();

    // Not an empty strip: nothing at all. A row of nothing above the dashboard
    // is a promise the user has not made.
    await waitFor(() => expect(view.toJSON()).toBeNull());
  });

  it('runs the scene and reports what happened', async () => {
    const calls = stubApi((url) =>
      url.includes('/activate') ? { scene_id: 's1', dispatched: 2, failed: [] } : [scene()],
    );
    const view = await renderStrip();

    await view.findByText('Good night');
    await fireEvent.press(view.getByLabelText('Run Good night'));

    await waitFor(() => expect(calls.some((c) => c.url.includes('/scenes/s1/activate'))).toBe(true));
    expect(await view.findByText('Done · 2 devices')).toBeTruthy();
  });

  it('says when part of a scene failed instead of claiming success', async () => {
    stubApi((url) =>
      url.includes('/activate')
        ? {
            scene_id: 's1',
            dispatched: 1,
            failed: [{ index: 1, device_id: 'd9', code: 'switch.power', message: 'device not found' }],
          }
        : [scene()],
    );
    const view = await renderStrip();

    await view.findByText('Good night');
    await fireEvent.press(view.getByLabelText('Run Good night'));

    expect(await view.findByText('1 done · 1 failed')).toBeTruthy();
  });

  it('says nothing happened, and why, when every action failed', async () => {
    stubApi((url) =>
      url.includes('/activate')
        ? {
            scene_id: 's1',
            dispatched: 0,
            failed: [{ index: 0, device_id: 'd9', code: 'switch.power', message: 'device not found' }],
          }
        : [scene()],
    );
    const view = await renderStrip();

    await view.findByText('Good night');
    await fireEvent.press(view.getByLabelText('Run Good night'));

    expect(await view.findByText('Nothing happened: device not found')).toBeTruthy();
  });
});
