import { render, fireEvent } from '@testing-library/react-native';
import { QueryClientProvider } from '@tanstack/react-query';
import { testQueryClient } from '../../test/query';

import { RadarPage } from './RadarPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';

/**
 * The radar, driven by the stand-in radio (`EXPO_PUBLIC_BLE_MOCK=1`, set in
 * src/test/env.ts). These are the real advert bytes and the real folding: two
 * products announce themselves every 1.2 s, and the list must hold two rows,
 * not forty.
 */

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

const catalog = [
  { product_key: 'hv_acme_bulb1', name: 'Acme Bulb', category: 'light', icon: null, hash: '60576964' },
  { product_key: 'hv_acme_plug1', name: 'Acme Smart Plug', category: 'plug', icon: null, hash: '8ecd4857' },
];

function stubApi() {
  const calls: { url: string; method: string; body?: string }[] = [];
  globalThis.fetch = jest.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, method: init?.method ?? 'GET', body: init?.body as string });

    if (url.includes('/catalog/products')) {
      // Answer only for the hashes asked about, like the real endpoint.
      const asked = decodeURIComponent(url.split('hashes=')[1] ?? '').split(',');
      return {
        ok: true,
        status: 200,
        text: async () => JSON.stringify(catalog.filter((p) => asked.includes(p.hash))),
      } as Response;
    }

    return { ok: true, status: 200, text: async () => JSON.stringify([home]) } as Response;
  }) as unknown as typeof fetch;

  return calls;
}

async function renderRadar() {
  const client = testQueryClient();
  return await render(
    <QueryClientProvider client={client}>
      <ThemeProvider>
        <RadarPage />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  useSession.setState({ access: 'token', homeId: 'home-1', refresh: null });
});

jest.setTimeout(15_000);

describe('radar', () => {
  it('names the devices it hears from the catalogue', async () => {
    stubApi();
    const view = await renderRadar();

    // The advert carries a hash; the name comes from the API. Showing a MAC
    // address here would be technically true and useless.
    expect(await view.findByText('Acme Bulb', {}, { timeout: 6_000 })).toBeTruthy();
    expect(await view.findByText('Acme Smart Plug', {}, { timeout: 6_000 })).toBeTruthy();
  });

  it('folds repeated adverts into one row per device', async () => {
    stubApi();
    const view = await renderRadar();

    await view.findByText('Acme Bulb', {}, { timeout: 6_000 });

    // Three advertising intervals later there must still be exactly one of each.
    await new Promise((resolve) => setTimeout(resolve, 3_800));

    expect(view.getAllByText('Acme Bulb')).toHaveLength(1);
    expect(view.getAllByText('Acme Smart Plug')).toHaveLength(1);
  });

  it('asks a picked device for the network instead of pairing blind', async () => {
    stubApi();
    const view = await renderRadar();

    await fireEvent.press(await view.findByText('Acme Bulb', {}, { timeout: 6_000 }));

    expect(await view.findByText('Which network should it join?')).toBeTruthy();
    expect(view.getByPlaceholderText('Network name')).toBeTruthy();
    expect(view.getByPlaceholderText('Password')).toBeTruthy();
  });
});
