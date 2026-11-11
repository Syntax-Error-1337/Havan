import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';

export type Room = {
  id: string;
  home_id: string;
  name: string;
  icon?: string | null;
  sort_order: number;
};

export function useRooms(homeId: string | undefined) {
  return useQuery({
    queryKey: [...qk.home(homeId ?? 'none'), 'rooms'],
    queryFn: () => request<Room[]>(`/homes/${homeId}/rooms`),
    enabled: Boolean(homeId),
    // Rooms are plumbing: they change when someone names a room, not often.
    staleTime: 5 * 60_000,
  });
}

export function useCreateRoom(homeId: string | undefined) {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (input: { name: string }) =>
      request<Room>(`/homes/${homeId}/rooms`, {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: [...qk.home(homeId ?? 'none'), 'rooms'] });
      // The dashboard's device list carries room names, so it is stale too.
      void qc.invalidateQueries({ queryKey: qk.devices(homeId ?? 'none') });
    },
  });
}
