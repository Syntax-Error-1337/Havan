import { fireEvent, render, waitFor } from '@testing-library/react-native';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { DeviceTile } from './DeviceTile';
import { ThemeProvider } from '../../../app-shell/theme';
import type { Device } from '../model/types';

/**
 * The tile's third state.
 *
 * "Online" is what the platform believes; a device that dropped off Wi-Fi an hour ago still believes it
 * is connected, and the dashboard used to render its last reading as if it were current. These tests pin
 * the three states a person sees, because the difference between them is the difference between "the
 * house is fine" and "that reading is from this morning".
 */

const base = {
  id: 'dev-1',
  vendor_id: 'v1',
  home_id: 'home-1',
  product_key: 'hv_acme_sensor1',
  name: 'Room Sensor',
  category: 'sensor',
  serial: 'SIL-1',
  room_id: null,
  points: [
    {
      id: 'p1',
      code: 'temperature.value',
      capability: 'temperature',
      slot: 'temperature',
      kind: 'number',
      writable: false,
      value: 21.5,
      unit: '°C',
    },
  ],
} as unknown as Device;

const minutesAgo = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();

async function renderTile(device: Device) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return await render(
    <QueryClientProvider client={client}>
      <ThemeProvider>
        <DeviceTile device={device} onPress={() => {}} />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

describe('a device that stopped reporting', () => {
  it('says when the reading was taken, not that the device is online', async () => {
    // "Online" is a fact about the socket. What a person needs from a sensor tile is the number and its
    // age, so the one line under the name answers the second question.
    const view = await renderTile({ ...base, online: true, last_seen_at: minutesAgo(1) });

    expect(view.getByText('Updated just now')).toBeTruthy();
    expect(view.queryByText('Silent')).toBeNull();
  });

  it('says how long, once, when the platform believes it is up and nothing has arrived for hours', async () => {
    const view = await renderTile({ ...base, online: true, last_seen_at: minutesAgo(240) });

    // The reading is still shown — it is the last thing the device said — and one line says how old it is,
    // so nobody reads 21.5 °C as this minute's temperature. There is no second "Silent" badge: the old
    // shape printed the same fact twice, in two vocabularies, on every tile in a house with a fault.
    expect(view.getByText('Silent for 4h')).toBeTruthy();
    expect(view.queryByText('Silent')).toBeNull();
    // The numeral and its unit are two elements now — `°C` sits a size smaller beside a 34-point reading — so the
    // test asks for the number and the unit rather than for a string the screen no longer contains. The sentence a
    // screen reader gets is still the whole "21.5 °C", and the test below holds that.
    expect(view.getByText('21.5')).toBeTruthy();
    expect(view.getByText('°C')).toBeTruthy();
  });

  it('is Offline and says how long, in one line', async () => {
    const view = await renderTile({ ...base, online: false, last_seen_at: minutesAgo(90) });

    expect(view.getByText('Offline · 2h ago')).toBeTruthy();
    expect(view.queryByText('Silent for 2h')).toBeNull();
  });

  it('tells a screen reader which of the three it is, and what it reads', async () => {
    const view = await renderTile({ ...base, online: true, last_seen_at: minutesAgo(240) });

    // The value is part of the label: the card's whole purpose is the number, and it used to be announced as
    // "Room Sensor, silent, Silent for 4h" — health without the reading it is about.
    expect(view.getByLabelText('Room Sensor, 21.5 °C, silent, Silent for 4h')).toBeTruthy();
  });

  it('says "off" or "on" for a switch rather than a boolean', async () => {
    const plug = {
      ...base,
      name: 'Coffee Plug',
      category: 'plug',
      online: true,
      last_seen_at: minutesAgo(1),
      points: [
        { code: 'switch.power', capability: 'switch', slot: 'power', kind: 'bool', writable: true, value: false },
      ],
    } as unknown as Device;

    const view = await renderTile(plug);
    expect(view.getByLabelText('Coffee Plug, Off, online')).toBeTruthy();
  });
});

describe('a command the device never answered', () => {
  /**
   * The half of a write that used to be silent.
   *
   * `POST /commands` answers `202 accepted`, the optimistic value shows, and then the device times out — the row falls
   * back to the old number and nothing says why. On this platform every write to a sleeping device does exactly that,
   * so the failure is the common case, not the edge.
   */
  const plug = (over: Partial<Record<string, unknown>> = {}) =>
    ({
      ...base,
      name: 'Coffee Plug',
      category: 'plug',
      online: true,
      last_seen_at: minutesAgo(1),
      points: [
        { code: 'switch.power', capability: 'switch', slot: 'power', kind: 'bool', writable: true, value: false },
        { code: 'energy.watts', capability: 'energy', slot: 'energy', kind: 'number', writable: false, value: 4 },
      ],
      ...over,
    }) as unknown as Device;

  function stubApi(status: string) {
    globalThis.fetch = jest.fn(async (url: string, init?: RequestInit) => {
      if (init?.method === 'POST') {
        return { ok: true, status: 202, text: async () => JSON.stringify({ command_id: 'c-1' }) } as Response;
      }
      if (String(url).includes('/commands/c-1')) {
        return { ok: true, status: 200, text: async () => JSON.stringify({ id: 'c-1', status }) } as Response;
      }
      return { ok: true, status: 200, text: async () => JSON.stringify([]) } as Response;
    }) as unknown as typeof fetch;
  }

  it('says the device did not answer, once the platform says so', async () => {
    stubApi('timeout');
    const view = await renderTile(plug());

    fireEvent.press(view.getByLabelText('Coffee Plug off'));

    expect(await view.findByText('No answer from the device — it may be offline.')).toBeTruthy();
  });

  it('says nothing while the command is still in flight', async () => {
    stubApi('dispatched');
    const view = await renderTile(plug());

    fireEvent.press(view.getByLabelText('Coffee Plug off'));

    // A message that appears the instant you press is a message about the press, not about the device.
    await waitFor(() => expect(view.queryByText(/No answer/)).toBeNull());
  });
});
