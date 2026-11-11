import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';
import type { Device } from './types';

export function useDevices(homeId: string | undefined) {
  return useQuery({
    queryKey: qk.devices(homeId ?? 'none'),
    enabled: Boolean(homeId),
    queryFn: () => request<{ data: Device[] }>(`/devices?home_id=${homeId}`).then((r) => r.data),
    // The dashboard is the app's home screen; keep it fresh without hammering.
    staleTime: 15_000,
  });
}

export function useDevice(id: string | undefined) {
  return useQuery({
    queryKey: qk.device(id ?? 'none'),
    enabled: Boolean(id),
    queryFn: () => request<Device>(`/devices/${id}`),
  });
}

/**
 * Optimistic on purpose: a toggle that waits for the cloud feels broken. The
 * command may still settle `rejected`, and then we roll back with the device's
 * own message.
 */
export function useSetPoint(deviceId: string) {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (input: { code: string; value: boolean | number | string }) =>
      request<{ command_id: string }>(`/devices/${deviceId}/commands`, {
        method: 'POST',
        body: JSON.stringify({ code: input.code, value: input.value }),
      }),
    onMutate: async (input) => {
      await qc.cancelQueries({ queryKey: qk.device(deviceId) });
      const previous = qc.getQueryData<Device>(qk.device(deviceId));
      if (previous) {
        qc.setQueryData<Device>(qk.device(deviceId), {
          ...previous,
          points: previous.points.map((p) =>
            p.code === input.code ? { ...p, value: input.value } : p,
          ),
        });
      }
      return { previous };
    },
    onError: (_err, _input, ctx) => {
      if (ctx?.previous) qc.setQueryData(qk.device(deviceId), ctx.previous);
    },
    onSettled: () => {
      // The websocket settles it; a refetch here covers the socket-missed case.
      qc.invalidateQueries({ queryKey: qk.device(deviceId) });
    },
  });
}
