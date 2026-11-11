import { render } from '@testing-library/react-native';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Text } from 'react-native';

import { useHomeStream } from './useHomeStream';
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
  open: () => void;
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

function Probe() {
  useHomeStream('home-1');
  return <Text>probe</Text>;
}

async function mount() {
  // No `gcTime: 0` here: nothing observes these entries, and an immediate gc
  // collects them before the assertion reads them.
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(qk.devices('home-1'), [device()]);
  client.setQueryData(qk.device('d1'), device());

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

  it('marks a device offline from the pulse', async () => {
    const { client } = await mount();

    lastSocket().emit({ t: 'device.offline', device_id: 'd1' });

    expect(client.getQueryData<Device[]>(qk.devices('home-1'))?.[0]?.online).toBe(false);
    expect(client.getQueryData<Device>(qk.device('d1'))?.online).toBe(false);
  });

  it('refetches after a settle, because the optimistic value was a guess', async () => {
    const { client } = await mount();
    const invalidate = jest.spyOn(client, 'invalidateQueries');

    lastSocket().emit({ t: 'command.settled', device_id: 'd1', code: 'switch.power' });

    expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.device('d1') });
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
});
