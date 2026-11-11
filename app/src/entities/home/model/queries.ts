import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';
import type { Home } from './types';

export function useHomes() {
  return useQuery({
    queryKey: qk.homes,
    queryFn: () => request<Home[]>('/homes'),
    // Homes change rarely; refetching on every screen focus is noise.
    staleTime: 60_000,
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
