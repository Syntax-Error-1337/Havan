import { useQuery } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';
import type { Device } from '../../device/model/types';
import type { Scene } from '../../scene/model/queries';
import type { Room } from './rooms';
import type { Home } from './types';

export type Dashboard = {
  home: Home;
  rooms: Room[];
  devices: Device[];
  scenes: Scene[];
  unread_count: number;
};

/**
 * The home screen, in one request.
 *
 * It used to be five — homes, devices, rooms, scenes, unread — and every one of them
 * could land at a different time, so the screen painted in stages and a radio round
 * trip was paid five times. The server composes it through the same services the
 * individual endpoints use, so tenancy rules are inherited rather than re-implemented.
 */
export function useDashboard(homeId: string | null) {
  return useQuery({
    queryKey: qk.dashboard(homeId ?? 'none'),
    queryFn: () => request<Dashboard>(`/homes/${homeId}/dashboard`),
    enabled: Boolean(homeId),
  });
}
