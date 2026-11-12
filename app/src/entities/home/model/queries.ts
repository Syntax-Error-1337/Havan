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
