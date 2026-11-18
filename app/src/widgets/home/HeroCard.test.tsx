import { fireEvent, render, waitFor } from '@testing-library/react-native';
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

/**
 * The ring.
 *
 * A gauge is for a value you can turn; a chart is for a value you can only watch. The demo data made the distinction
 * concrete: a sensor's temperature has a range of −10…50 and is read-only, so a ring would say "18.3 is 47 % of a
 * range nobody chose", while a lamp's brightness (1…100, writable) is exactly the thing a person turns.
 */

const lamp = (over: Partial<Device> = {}): Device =>
  ({
    id: 'lamp-1',
    vendor_id: 'v1',
    home_id: 'h1',
    room_id: null,
    room_name: 'Living Room',
    name: 'Floor Lamp',
    category: 'light',
    product_key: 'hv_lamp',
    serial: 'LAMP-1',
    firmware: null,
    online: true,
    last_seen_at: new Date().toISOString(),
    report_interval_secs: 60,
    points: [
      { code: 'brightness.level', capability: 'brightness', value: 80, unit: '%', writable: true, min: 1, max: 100 },
      { code: 'switch.power', capability: 'switch', value: true, unit: null, writable: true, min: null, max: null },
    ],
    ...over,
  }) as unknown as Device;

function stubEmptyHistoryWith(fetchSpy?: (body: unknown) => void) {
  globalThis.fetch = jest.fn(async (url: string, init?: RequestInit) => {
    if (init?.body) fetchSpy?.(JSON.parse(String(init.body)));
    return {
      ok: true,
      status: 200,
      text: async () =>
        String(url).includes('/commands')
          ? JSON.stringify({ accepted: true })
          : JSON.stringify({ buckets: [], bucket_secs: 3600, window: '1d' }),
    } as Response;
  }) as unknown as typeof fetch;
}

describe('hero card dial', () => {
  it('draws a ring for a value that can be turned, and no chart under it', async () => {
    stubEmptyHistoryWith();
    const view = await renderCard(lamp());

    expect(await view.findByText('80')).toBeTruthy();
    // The range ends are off for a percentage: "80 %" already says 80 of 100.
    expect(view.queryByText('1')).toBeNull();
    // One data moment per card: a dial has nothing to say about yesterday.
    expect(view.queryByText('No history for this reading yet.')).toBeNull();
  });

  it('moves in fives, and posts the value the customer asked for', async () => {
    const bodies: unknown[] = [];
    stubEmptyHistoryWith((b) => bodies.push(b));
    const view = await renderCard(lamp());

    fireEvent.press(await view.findByLabelText('Increase brightness'));

    await waitFor(() => expect(bodies.length).toBeGreaterThan(0));
    expect(bodies[0]).toEqual({ code: 'brightness.level', value: 85 });
  });

  it('does not offer a dial on a device that is not reporting', async () => {
    // A dial you cannot turn is a lie. Offline, the ring is replaced by the plain number — no steppers to press and
    // nothing that looks adjustable, because the platform cannot deliver the command.
    const bodies: unknown[] = [];
    stubEmptyHistoryWith((b) => bodies.push(b));
    const view = await renderCard(lamp({ online: false }));

    expect(view.queryByLabelText('Increase brightness')).toBeNull();
    expect(view.queryByLabelText('Decrease brightness')).toBeNull();
    expect(view.getByText('80')).toBeTruthy();
    expect(bodies).toEqual([]);
  });
});
