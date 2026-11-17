import { useQueries, useQuery } from '@tanstack/react-query';

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
 * The most the server will return in one page. `device_limit` is clamped to this on the way in, so a
 * client that asks for more is not refused — it is silently given 500, which is how the first version of
 * the paging footer ended up requesting 800, receiving the same first 500 twice, and making every device
 * past 500 unreachable on a 1,200-device home.
 */
export const DEVICE_PAGE_MAX = 500;

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

/**
 * The pages after the first, one request each, by cursor.
 *
 * The first page has a fixed size, so "show me more" cannot be an ever-larger `device_limit`: the server
 * clamps at `DEVICE_PAGE_MAX`, and past that every press re-fetched the same first page while the footer
 * kept promising the rest. A cursor is the only thing that moves forward.
 *
 * Each page keeps its own cache entry under the dashboard's key prefix, so the live stream patches them
 * with the same `setQueriesData` it uses for the first page, and a reconnect invalidates all of them.
 */
export function useDashboardTail(homeId: string | null, cursors: string[]) {
  return useQueries({
    queries: cursors.map((cursor) => ({
      queryKey: [...qk.dashboard(homeId ?? 'none'), 'page', cursor] as const,
      queryFn: () =>
        request<Dashboard>(
          `/homes/${homeId}/dashboard?device_limit=${DEVICE_PAGE_MAX}&device_cursor=${encodeURIComponent(cursor)}`,
        ),
      enabled: Boolean(homeId),
    })),
  });
}
