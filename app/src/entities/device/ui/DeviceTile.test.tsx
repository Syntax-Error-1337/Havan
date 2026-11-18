import { render } from '@testing-library/react-native';
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
    expect(view.getByText('21.5 °C')).toBeTruthy();
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
