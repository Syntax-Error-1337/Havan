import { render, fireEvent } from '@testing-library/react-native';
import { QueryClientProvider } from '@tanstack/react-query';
import { testQueryClient } from '../../test/query';

import { PhoneHandoff, RadarPage } from './RadarPage';
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

/**
 * The screen a customer without a radio in their browser meets first. It used to be two lines — "No radio
 * here / This build cannot scan for Bluetooth devices" — under a heading promising an add-device flow, on
 * an otherwise empty page. It is a handoff now, and this test holds the parts that make it one.
 */
describe('a browser with no Bluetooth', () => {
  it('hands off to the phone instead of stopping dead', async () => {
    const client = testQueryClient();
    const view = await render(
      <QueryClientProvider client={client}>
        <ThemeProvider>
          <PhoneHandoff />
        </ThemeProvider>
      </QueryClientProvider>,
    );

    expect(view.getByText('Pair from your phone')).toBeTruthy();
    // The reason, in the customer's language — not "this build cannot scan".
    expect(view.getByText(/cannot scan for Bluetooth/)).toBeTruthy();
    expect(view.queryByText('No radio here')).toBeNull();
    // Three steps, and a way out that does not depend on the phone being in the room.
    expect(view.getByText(/Open Haven on the phone/)).toBeTruthy();
    expect(view.getByText(/powered on and close/)).toBeTruthy();
    expect(view.getByText(/it will appear within a few seconds/)).toBeTruthy();
    expect(view.getByText('Back to your home')).toBeTruthy();
  });
});

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
    // Labelled, not placeholder-only, and found the way a person finds them.
    expect(view.getByLabelText('Network name')).toBeTruthy();
    expect(view.getByLabelText('Wi-Fi password')).toBeTruthy();

    // The two reasons a first pairing fails, said before it happens: a 5 GHz-only network, and a password
    // typed blind on a phone keyboard.
    expect(view.getByText(/2\.4 GHz only/)).toBeTruthy();
    const password = view.getByLabelText('Wi-Fi password');
    expect(password.props.secureTextEntry).toBe(true);
    await fireEvent.press(view.getByLabelText('Show Wi-Fi password'));
    // Wait for the re-render rather than reading the node from the frame the press happened in.
    expect(
      (await view.findByLabelText('Hide Wi-Fi password')) !== null,
    ).toBe(true);
    expect(view.getByLabelText('Wi-Fi password').props.secureTextEntry).toBe(false);
  });
});
