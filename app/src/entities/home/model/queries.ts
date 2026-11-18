import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';
import type { Home } from './types';

/**
 * The account's homes.
 *
 * `enabled` exists because the dashboard already returns the home it is showing: with
 * a stored home id this call is a round trip that changes nothing. It still runs when
 * the app has to decide *which* home to open — the first launch on a new account.
 */
export function useHomes(onlyWithoutHome = false) {
  return useQuery({
    queryKey: qk.homes,
    queryFn: () => request<Home[]>('/homes'),
    enabled: !onlyWithoutHome,
    // Homes change rarely; refetching on every screen focus is noise.
    staleTime: 60_000,
  });
}

/**
 * Move the home to another timezone.
 *
 * Every schedule in the home is read against this, so it is one query with one cache invalidation —
 * a home screen still showing yesterday's zone is a schedule that looks right and fires wrong.
 */
export function useSetTimeZone() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { id: string; timezone: string }) =>
      request<void>(`/homes/${input.id}`, {
        method: 'PATCH',
        body: JSON.stringify({ timezone: input.timezone }),
      }),
    onSuccess: (_data, input) => {
      void qc.invalidateQueries({ queryKey: qk.homes });
      void qc.invalidateQueries({ queryKey: qk.dashboard(input.id) });
    },
  });
}

/**
 * Where the home is, for the rules that talk about sunrise and sunset.
 *
 * `null` clears it: both coordinates are sent as null together, because the service refuses a location with only
 * one of them — and a customer who moves house should be able to say "not any more".
 */
export function useSetLocation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { id: string; lat: number | null; lon: number | null }) =>
      request<void>(`/homes/${input.id}`, {
        method: 'PATCH',
        body: JSON.stringify({ geo_lat: input.lat, geo_lon: input.lon }),
      }),
    onSuccess: (_data, input) => {
      void qc.invalidateQueries({ queryKey: qk.homes });
      void qc.invalidateQueries({ queryKey: qk.dashboard(input.id) });
    },
  });
}

export function useRenameHome() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { id: string; name: string }) =>
      request<void>(`/homes/${input.id}`, {
        method: 'PATCH',
        body: JSON.stringify({ name: input.name }),
      }),
    onSuccess: (_data, input) => {
      void qc.invalidateQueries({ queryKey: qk.homes });
      void qc.invalidateQueries({ queryKey: qk.dashboard(input.id) });
    },
  });
}

export function useCreateHome() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { name: string; timezone: string }) =>
      request<Home>('/homes', { method: 'POST', body: JSON.stringify(input) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.homes }),
  });
}
