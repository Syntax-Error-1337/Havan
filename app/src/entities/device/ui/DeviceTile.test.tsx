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
  it('is Online when it reported a minute ago', async () => {
    const view = await renderTile({ ...base, online: true, last_seen_at: minutesAgo(1) });

    expect(view.getByText('Online')).toBeTruthy();
    expect(view.queryByText('Silent')).toBeNull();
  });

  it('is Silent, not Online, when the platform believes it is up but has heard nothing for hours', async () => {
    const view = await renderTile({ ...base, online: true, last_seen_at: minutesAgo(240) });

    expect(view.getByText('Silent')).toBeTruthy();
    expect(view.queryByText('Online')).toBeNull();
    // The reading is still shown — it is the last thing the device said — but the line under it says
    // how old it is, so nobody reads 21.5 °C as this minute's temperature.
    expect(view.getByText('Silent for 4h')).toBeTruthy();
    expect(view.getByText('21.5 °C')).toBeTruthy();
  });

  it('is Offline when the platform says so, and says how long it has been', async () => {
    const view = await renderTile({ ...base, online: false, last_seen_at: minutesAgo(90) });

    expect(view.getByText('Offline')).toBeTruthy();
    expect(view.getByText('Silent for 2h')).toBeTruthy();
  });

  it('tells a screen reader which of the three it is', async () => {
    const view = await renderTile({ ...base, online: true, last_seen_at: minutesAgo(240) });

    expect(view.getByLabelText('Room Sensor, silent, Silent for 4h')).toBeTruthy();
  });
});
