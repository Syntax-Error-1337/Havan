import { render } from '@testing-library/react-native';
import { QueryClientProvider } from '@tanstack/react-query';
import { testQueryClient } from '../../test/query';

import { HeroCard } from './HeroCard';
import { ThemeProvider } from '../../app-shell/theme';
import type { Device } from '../../entities/device';

/**
 * The card the dashboard leads with, which had no test at all.
 *
 * That is how it shipped a state that said the same thing twice, in two voices — "No history for this reading yet."
 * with "no readings yet" directly under it, and "steady" beside them describing a day that held no data. The empty
 * state is the one state a sensor card spends most of its life in, and nothing was checking it.
 */

const sensor = (over: Partial<Device> = {}): Device =>
  ({
    id: 'd1',
    vendor_id: 'v1',
    home_id: 'h1',
    room_id: null,
    room_name: 'Hallway',
    name: 'Hallway Sensor',
    category: 'sensor',
    product_key: 'hv_sensor',
    serial: 'ESP12E-001',
    firmware: null,
    online: false,
    last_seen_at: new Date(Date.now() - 86_400_000).toISOString(),
    report_interval_secs: 3600,
    points: [
      {
        code: 'temperature',
        capability: 'temperature',
        value: 18.3,
        unit: '°C',
        writable: false,
        min: null,
        max: null,
      },
    ],
    ...over,
  }) as unknown as Device;

function stubEmptyHistory() {
  globalThis.fetch = jest.fn(async () => ({
    ok: true,
    status: 200,
    text: async () => JSON.stringify({ buckets: [], bucket_secs: 3600, window: '1d' }),
  })) as unknown as typeof fetch;
}

async function renderCard(device: Device) {
  return await render(
    <QueryClientProvider client={testQueryClient()}>
      <ThemeProvider>
        <HeroCard device={sensor(device)} />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

describe('hero card', () => {
  it('says the one thing there is to say when a reading has no history', async () => {
    stubEmptyHistory();
    const view = await renderCard(sensor());

    expect(await view.findByText('No history for this reading yet.')).toBeTruthy();
    // The second sentence for the same fact, and the claim about a day with no readings in it.
    expect(view.queryByText('no readings yet')).toBeNull();
    expect(view.queryByText('steady')).toBeNull();
  });

  it('still shows the reading, which is the one fact that exists', async () => {
    stubEmptyHistory();
    const view = await renderCard(sensor());

    expect(await view.findByText('18.3')).toBeTruthy();
    expect(view.getByText('°C')).toBeTruthy();
  });
});
