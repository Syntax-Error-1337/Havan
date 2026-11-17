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
  /** Present only when the request was paged: the home's count, not the page's. */
  devices_total?: number;
  /** Present only when there is another page behind this one. */
  devices_next_cursor?: string;
};

/** How many devices one dashboard request asks for. */
export const DEVICE_PAGE = 200;

/**
 * The home screen, in one request.
 *
 * It used to be five — homes, devices, rooms, scenes, unread — and every one of them
 * could land at a different time, so the screen painted in stages and a radio round
 * trip was paid five times. The server composes it through the same services the
 * individual endpoints use, so tenancy rules are inherited rather than re-implemented.
 *
 * The request asks for a page of devices (`device_limit`). A hundred devices is 322 kB of JSON;
 * a thousand is about 3 MB, for a screen that shows ten of them. Most homes fit inside one page and
 * never see a cursor; the ones that do not get a footer that asks for the rest, which is a person
 * deciding to spend the bytes rather than a phone spending them on every cold start.
 */
export function useDashboard(homeId: string | null, deviceLimit = DEVICE_PAGE) {
  return useQuery({
    queryKey: [...qk.dashboard(homeId ?? 'none'), deviceLimit] as const,
    queryFn: () =>
      request<Dashboard>(`/homes/${homeId}/dashboard?device_limit=${deviceLimit}`),
    enabled: Boolean(homeId),
  });
}
