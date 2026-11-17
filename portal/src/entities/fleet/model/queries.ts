import { useInfiniteQuery, useQuery } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';

export type FleetDevice = {
  id: string;
  product_key: string;
  product_name: string;
  name: string;
  serial: string;
  state: string;
  bound: boolean;
  online: boolean;
  fw_version: string | null;
  last_seen_at: string | null;
  created_at: string;
};

export type FleetPage = {
  devices: FleetDevice[];
  next_cursor: string | null;
};

/**
 * The most the server will return in one page (`FleetFilter::limit` clamps at 200).
 *
 * The page used to ask for the default fifty, which is a reasonable page for a web table and a bad one
 * for a fleet: a vendor with 1 900 devices needed thirty-eight presses to see them all.
 */
export const FLEET_PAGE_SIZE = 200;

export type FleetFilter = {
  product_key?: string;
  state?: 'online' | 'offline' | 'bound' | 'unbound';
};

function query(filter: FleetFilter, cursor?: string | null): string {
  const params = new URLSearchParams();
  params.set('limit', String(FLEET_PAGE_SIZE));
  if (filter.product_key) params.set('product_key', filter.product_key);
  if (filter.state) params.set('state', filter.state);
  if (cursor) params.set('cursor', cursor);
  const qs = params.toString();
  return qs ? `?${qs}` : '';
}

/**
 * The fleet, a page at a time.
 *
 * Keyset pagination, not offsets: a device claiming while somebody reads page two
 * must not push a row from page one onto page two, where it would appear twice and
 * be counted once.
 */
export function useFleet(filter: FleetFilter) {
  return useInfiniteQuery({
    queryKey: ['fleet', filter],
    queryFn: ({ pageParam }) =>
      request<FleetPage>(`/vendor/devices${query(filter, pageParam)}`),
    initialPageParam: null as string | null,
    getNextPageParam: (last: FleetPage) => last.next_cursor,
  });
}

export type FleetStats = {
  devices: { total: number; online: number; bound: number; unbound: number };
  new_devices: { last_7d: number; last_30d: number };
  by_product: { product_key: string; name: string; total: number; online: number }[];
  firmware: { version: string | null; devices: number }[];
};

export function useFleetStats() {
  return useQuery({
    queryKey: ['fleet', 'stats'],
    queryFn: () => request<FleetStats>('/vendor/analytics'),
    staleTime: 60_000,
  });
}
