import { act, render } from '@testing-library/react-native';
import { AppState } from 'react-native';
import { QueryClientProvider } from '@tanstack/react-query';
import { testQueryClient } from '../../test/query';
import { Text } from 'react-native';

import { retryDelayMs, useHomeStream } from './useHomeStream';
import { qk } from '../../shared/api/keys';
import { useSession } from '../../entities/session';
import type { Device } from '../../entities/device/model/types';

/**
 * The stream writes into the query cache. That is the one place a wrong key
 * fails silently: the socket is healthy, the message arrives, and the screen
 * simply never changes.
 */

type TestSocket = {
  url: string;
  sent: string[];
  closed: boolean;
  readyState: number;
  open: () => void;
  /** The network drops it: closed, and the client is told — what a lost cell tower looks like. */
  terminate: () => void;
  emit: (message: unknown) => void;
  emitRaw: (data: string) => void;
};

const lastSocket = (): TestSocket =>
  (globalThis as unknown as { __lastSocket: () => TestSocket }).__lastSocket();

const resetSockets = () =>
  (globalThis as unknown as { __resetSockets: () => void }).__resetSockets();

const device = (over: Partial<Device> = {}): Device => ({
  id: 'd1',
  name: 'Lamp',
  serial: 'S1',
  product_key: 'k',
  category: 'light',
  online: true,
  points: [
    {
      id: 'p1',
      code: 'switch.power',
      capability: 'switch',
      slot: 'power',
      kind: 'bool',
      writable: true,
      value: false,
    },
  ],
  ...over,
});

const DASHBOARD_KEY = [...qk.dashboard('home-1'), 200] as const;

function Probe() {
  useHomeStream('home-1');
  return <Text>probe</Text>;
}

async function mount() {
  // No `gcTime: 0` here: nothing observes these entries, and an immediate gc
  // collects them before the assertion reads them.
  const client = testQueryClient();
  client.setQueryData(qk.devices('home-1'), [device()]);
  client.setQueryData(qk.device('d1'), device());
  // The real key carries the page size; the dashboard reads *this* entry, so a test that seeds the
  // bare prefix would pass while the running app stayed still — which is exactly what happened.
  client.setQueryData(DASHBOARD_KEY, {
    devices: [device()],
    unread_count: 0,
    rooms: [],
    scenes: [],
  });

  const view = await render(
    <QueryClientProvider client={client}>
      <Probe />
    </QueryClientProvider>,
  );

  return { client, view };
}

beforeEach(() => {
  resetSockets();
  useSession.setState({ access: 'a-token', homeId: 'home-1', refresh: null });
});

describe('home stream', () => {
  it('authenticates with the token and the home it wants', async () => {
    await mount();

    const socket = lastSocket();
    expect(socket.url).toMatch(/^ws:\/\/.+\/stream$/);

    socket.open();
    expect(JSON.parse(socket.sent[0] ?? '{}')).toEqual({
      t: 'auth',
      token: 'a-token',
      homes: ['home-1'],
    });
  });

  it('pushes a point change into both caches', async () => {
    const { client } = await mount();

    lastSocket().emit({
      t: 'point.changed',
      device_id: 'd1',
      code: 'switch.power',
      value: true,
    });

    const list = client.getQueryData<Device[]>(qk.devices('home-1'));
    expect(list?.[0]?.points[0]?.value).toBe(true);

    // The detail screen holds its own key; a cache that updates one and not the
    // other shows a device that disagrees with itself.
    const detail = client.getQueryData<Device>(qk.device('d1'));
    expect(detail?.points[0]?.value).toBe(true);
  });

  it('marks a device offline from the pulse, in the entry the dashboard reads', async () => {
    const { client } = await mount();

    lastSocket().emit({ t: 'device.offline', device_id: 'd1' });

    expect(client.getQueryData<Device[]>(qk.devices('home-1'))?.[0]?.online).toBe(false);
    expect(client.getQueryData<Device>(qk.device('d1'))?.online).toBe(false);
    // The screen renders `dashboard.devices`, not the devices cache.
    expect(client.getQueryData<{ devices: Device[] }>(DASHBOARD_KEY)?.devices[0]?.online).toBe(
      false,
    );
  });

  it('refetches after a settle, because the optimistic value was a guess', async () => {
    const { client } = await mount();
    const invalidate = jest.spyOn(client, 'invalidateQueries');

    lastSocket().emit({ t: 'command.settled', device_id: 'd1', code: 'switch.power' });

    expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.device('d1') });
  });

  it('bumps the alert count when a pushed notification arrives, without a device', async () => {
    const { client } = await mount();
    // A notification frame names a person, not a device: no `device_id` at all.
    client.setQueryData(qk.unread, { unread: 2 });
    client.setQueryData(qk.dashboard('home-1'), {
      home: { id: 'home-1', name: 'Workshop' },
      devices: [],
      unread_count: 2,
    });
    const invalidate = jest.spyOn(client, 'invalidateQueries');

    lastSocket().emit({ t: 'notification', id: 'n1' });

    expect(client.getQueryData<{ unread: number }>(qk.unread)?.unread).toBe(3);
    expect(
      client.getQueryData<{ unread_count: number }>(qk.dashboard('home-1'))?.unread_count,
    ).toBe(3);
    // The list is refetched, but the count did not wait for it.
    expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.notifications });
  });

  it('ignores noise instead of crashing the screen', async () => {
    const { client } = await mount();
    const socket = lastSocket();

    socket.emitRaw('not json');
    socket.emit({ t: 'something.new', device_id: 'd1' });
    socket.emit({ t: 'point.changed' }); // no device, no code

    expect(client.getQueryData<Device[]>(qk.devices('home-1'))?.[0]?.points[0]?.value).toBe(false);
  });

  it('closes the socket when the screen goes away', async () => {
    const { view } = await mount();
    const socket = lastSocket();

    await view.unmount();

    expect(socket.closed).toBe(true);
  });

  /**
   * A socket that never comes back works in a demo and dies on a train. These tests are about the two
   * things a real phone does to a connection: lose it, and suspend it.
   *
   * The retry *timer* is checked through `retryDelayMs` rather than by advancing a fake clock: mounting
   * a component under jest's fake timers leaves the next mount in this file without an effect at all,
   * which is an artefact of the test environment rather than anything a customer would meet.
   */
  it('says reconnecting the moment the connection drops, and does not spin', async () => {
    const { view } = await mountWithStatus();
    const socket = lastSocket();
    await act(async () => {
      socket.open();
    });
    expect(view.getByText('live')).toBeTruthy();

    await act(async () => {
      socket.terminate();
    });

    expect(view.getByText('reconnecting')).toBeTruthy();
    // No hot loop: the next socket is a scheduled retry, not an immediate new connection.
    expect(lastSocket()).toBe(socket);
  });

  it('opens another socket when the app comes back, and refetches what it held', async () => {
    const listeners: ((state: string) => void)[] = [];
    const spy = jest
      .spyOn(AppState, 'addEventListener')
      .mockImplementation(((_: string, handler: (state: string) => void) => {
        listeners.push(handler);
        return { remove: () => undefined } as never;
      }) as never);

    try {
      const { view, client } = await mountWithStatus();
      const invalidate = jest.spyOn(client, 'invalidateQueries');
      const first = lastSocket();
      await act(async () => {
        first.open();
      });

      // The phone went to the background, which is what suspends the socket on a real device.
      await act(async () => {
        first.terminate();
      });
      expect(listeners).toHaveLength(1);

      // The customer unlocks it long before the backoff timer would have fired.
      await act(async () => {
        listeners[0]?.('active');
      });

      const second = lastSocket();
      expect(second).not.toBe(first);

      // Being back is not being up to date: the window spent disconnected may have hidden changes,
      // so the resync happens when the new socket is open, not when it is created.
      expect(invalidate).not.toHaveBeenCalled();
      await act(async () => {
        second.open();
      });
      expect(view.getByText('live')).toBeTruthy();
      expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.dashboard('home-1') });
      expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.devices('home-1') });
    } finally {
      spy.mockRestore();
    }
  });

  it('does not reconnect while the app is already connected', async () => {
    const listeners: ((state: string) => void)[] = [];
    const spy = jest
      .spyOn(AppState, 'addEventListener')
      .mockImplementation(((_: string, handler: (state: string) => void) => {
        listeners.push(handler);
        return { remove: () => undefined } as never;
      }) as never);

    try {
      await mountWithStatus();
      const socket = lastSocket();
      await act(async () => {
        socket.open();
      });

      await act(async () => {
        listeners[0]?.('active');
      });

      // A resume event on a healthy socket must not throw the subscription away and re-auth.
      expect(lastSocket()).toBe(socket);
      expect(socket.closed).toBe(false);
    } finally {
      spy.mockRestore();
    }
  });

  it('backs off further each time, and stops at half a minute', () => {
    expect(retryDelayMs(0)).toBe(500);
    expect(retryDelayMs(1)).toBe(1000);
    expect(retryDelayMs(2)).toBe(2000);
    expect(retryDelayMs(6)).toBe(30_000);
    // A phone that has been offline for an hour must still retry — with a ceiling, not a growing wait.
    expect(retryDelayMs(50)).toBe(30_000);
    expect(retryDelayMs(-1)).toBe(500);
  });
});

/** A probe that renders the stream status, so a state the customer can see can be asserted. */
async function mountWithStatus() {
  function StatusProbe() {
    return <Text>{useHomeStream('home-1')}</Text>;
  }

  const client = testQueryClient();
  client.setQueryData(qk.devices('home-1'), [device()]);
  client.setQueryData(qk.device('d1'), device());

  const view = await render(
    <QueryClientProvider client={client}>
      <StatusProbe />
    </QueryClientProvider>,
  );

  return { client, view };
}
