import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';

/**
 * A room, as the API sends it. The field is `sort`, not `sort_order`: the app type
 * used to disagree with the server, which is the kind of typo that silently orders
 * a list by `undefined` forever.
 */
export type Room = {
  id: string;
  home_id: string;
  name: string;
  icon?: string | null;
  sort: number;
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
      // The dashboard carries both the room list and the device→room mapping.
      void qc.invalidateQueries({ queryKey: qk.devices(homeId ?? 'none') });
      void qc.invalidateQueries({ queryKey: qk.dashboard(homeId ?? 'none') });
    },
  });
}

export function useRenameRoom(homeId: string | undefined) {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (input: { id: string; name: string }) =>
      request<void>(`/rooms/${input.id}?home_id=${homeId}`, {
        method: 'PATCH',
        body: JSON.stringify({ name: input.name }),
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: [...qk.home(homeId ?? 'none'), 'rooms'] });
      void qc.invalidateQueries({ queryKey: qk.dashboard(homeId ?? 'none') });
    },
  });
}

export function useDeleteRoom(homeId: string | undefined) {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (roomId: string) =>
      request<void>(`/rooms/${roomId}?home_id=${homeId}`, { method: 'DELETE' }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: [...qk.home(homeId ?? 'none'), 'rooms'] });
      // Devices in that room fall back to "unassigned", so the board changed too.
      void qc.invalidateQueries({ queryKey: qk.dashboard(homeId ?? 'none') });
    },
  });
}
