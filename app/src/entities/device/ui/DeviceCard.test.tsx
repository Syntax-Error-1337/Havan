import { fireEvent, render, waitFor } from '@testing-library/react-native';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { DeviceCard } from './DeviceCard';
import { ThemeProvider } from '../../../app-shell/theme';
import type { Device } from '../model/types';

const device: Device = {
  id: 'dev-1',
  vendor_id: 'v1',
  home_id: 'home-1',
  product_key: 'hv_acme_bulb1',
  name: 'Hallway bulb',
  category: 'light',
  online: true,
  room_id: null,
  points: [
    {
      code: 'switch.power',
      kind: 'bool',
      value: true,
      writable: true,
    },
  ],
} as unknown as Device;

async function renderCard() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return await render(
    <QueryClientProvider client={client}>
      <ThemeProvider>
        <DeviceCard device={device} onPress={() => {}} />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

describe('a device row whose command failed', () => {
  it('says why, in the row, instead of flipping back in silence', async () => {
    // The broker is down: the API answers 503 and refuses rather than pretending.
    const fetchMock = jest.fn(async () =>
      new Response(
        JSON.stringify({
          error: {
            code: 'unavailable',
            message: 'the device broker is unavailable; command 01a1 was not sent',
          },
        }),
        { status: 503, headers: { 'content-type': 'application/json' } },
      ),
    );
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    const view = await renderCard();
    const toggle = view.getByLabelText('Hallway bulb on');
    await fireEvent(toggle, 'valueChange', false);

    await waitFor(() => expect(view.getByText(/Hub unreachable/)).toBeTruthy());
    // The point code is what the row normally shows; the failure replaces it rather than
    // stacking a second line nobody reads.
    expect(view.queryByText(/switch\.power/)).toBeNull();
  });
});

describe('a bool point that has never reported a value', () => {
  it('sends true, not a number', async () => {
    // This is the bug the browser found: `value: null` made the toggle infer "off", and
    // "off" for a non-boolean was 0 — which the server rejects with
    // "'switch.power' expects a bool value". The kind decides the value, not the current one.
    const calls: { url: string; body?: string }[] = [];
    const fetchMock = jest.fn(async (url: string, init?: RequestInit) => {
      calls.push({ url: String(url), body: init?.body as string });
      return new Response(JSON.stringify({ command_id: 'c1', status: 'queued' }), {
        status: 202,
        headers: { 'content-type': 'application/json' },
      });
    });
    globalThis.fetch = fetchMock as unknown as typeof fetch;

    const never = {
      ...device,
      points: [{ code: 'switch.power', kind: 'bool', value: null, writable: true }],
    } as unknown as Device;

    const client = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const view = await render(
      <QueryClientProvider client={client}>
        <ThemeProvider>
          <DeviceCard device={never} onPress={() => {}} />
        </ThemeProvider>
      </QueryClientProvider>,
    );

    await fireEvent(view.getByLabelText('Hallway bulb off'), 'valueChange', true);

    await waitFor(() => expect(calls.length).toBeGreaterThan(0));
    const body = JSON.parse(calls[0]?.body ?? '{}');
    expect(body.value).toBe(true);
    expect(typeof body.value).toBe('boolean');
  });
});
