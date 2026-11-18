import { renderHook, waitFor, act } from '@testing-library/react-native';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';

import { useSetPoint } from './queries';
import { qk } from '../../../shared/api/keys';
import type { Device } from './types';

/**
 * The optimistic write, at the cache that is actually rendered.
 *
 * The dashboard's tiles come from the `['home', id, 'dashboard']` aggregate, not from `['device', id]`. The
 * mutation patched only the latter, so the switch under a customer's thumb did not move until the socket echoed
 * back. A test on the rendered tile cannot catch that — the tile takes its device as a prop — so the assertion
 * belongs on the cache.
 */

const home = 'home-1';
const lamp = {
  id: 'dev-lamp',
  home_id: home,
  name: 'Bedside Bulb',
  category: 'light',
  online: true,
  points: [
    { code: 'switch.power', capability: 'switch', slot: 'power', kind: 'bool', writable: true, value: false },
  ],
} as unknown as Device;

function wrapper(client: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

describe('useSetPoint', () => {
  it('moves the switch in the dashboard aggregate, not only in the device cache', async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    client.setQueryData(qk.device(lamp.id), lamp);
    client.setQueryData(qk.dashboard(home), {
      devices: [lamp],
      presence: { total: 1, online: 1, silent: 0 },
    });
    client.setQueryData(qk.devices(home), [lamp]);

    // Never resolves: the point of the test is the state before any answer.
    const original = globalThis.fetch;
    (globalThis as unknown as { fetch: unknown }).fetch = () => new Promise(() => {});

    try {
      const { result } = await renderHook(() => useSetPoint(lamp.id), { wrapper: wrapper(client) });
      await act(async () => {
        result.current.mutate({ code: 'switch.power', value: true });
      });

      await waitFor(() => {
        const dashboard = client.getQueryData<{ devices: Device[] }>(qk.dashboard(home));
        expect(dashboard?.devices[0]?.points[0]?.value).toBe(true);
      });
      // And the plain list, for any screen that reads that shape.
      const list = client.getQueryData<Device[]>(qk.devices(home));
      expect(list?.[0]?.points[0]?.value).toBe(true);
      // And the single device, which was the only one that ever worked.
      const single = client.getQueryData<Device>(qk.device(lamp.id));
      expect(single?.points[0]?.value).toBe(true);
    } finally {
      (globalThis as unknown as { fetch: unknown }).fetch = original;
    }
  });
});
