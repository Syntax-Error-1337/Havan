import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';
import type { Device } from './types';

export function useDevices(homeId: string | undefined) {
  return useQuery({
    queryKey: qk.devices(homeId ?? 'none'),
    enabled: Boolean(homeId),
    // The API returns a bare array. It does not wrap in `{data}` — an earlier
    // version of this line assumed it did, and the dashboard silently rendered
    // "no devices" for every home.
    queryFn: () => request<Device[]>(`/devices?home_id=${homeId}`),
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
      // The home screen shows the same points; without this it keeps a value the
      // device has already moved past (no socket, e.g. when offline).
      const homeId = qc.getQueryData<Device>(qk.device(deviceId))?.home_id;
      if (homeId) qc.invalidateQueries({ queryKey: qk.dashboard(homeId) });
    },
  });
}

/**
 * Give a device the name the household actually uses for it.
 *
 * "Acme Smart Plug (2)" is what the box said; "Coffee machine" is what the person says. The server has
 * accepted this since the first PATCH — the app simply never offered it, so every customer's home was
 * full of product names.
 */
export function useRenameDevice() {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (input: { deviceId: string; name: string }) =>
      request<Device>(`/devices/${input.deviceId}`, {
        method: 'PATCH',
        body: JSON.stringify({ name: input.name }),
      }),
    onSuccess: (device) => {
      qc.setQueryData(qk.device(device.id), device);
      // The list and every cached dashboard page carry the old name; one prefix update fixes them all.
      void qc.invalidateQueries({ queryKey: qk.devices(device.home_id ?? 'none') });
      if (device.home_id) qc.invalidateQueries({ queryKey: qk.dashboard(device.home_id) });
    },
  });
}

/**
 * Release a device from this home.
 *
 * The server calls it `unclaim`: the row stays (so the licence activation and the history receipts stay
 * honest) but the device no longer belongs to anyone, and it must be paired again before it works in
 * this or any home. Without this the app had no answer to "I gave that bulb away" — the tile sat in the
 * list saying "Silent for 12d" forever.
 */
export function useRemoveDevice() {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (input: { deviceId: string; homeId: string }) =>
      request<void>(`/devices/${input.deviceId}`, { method: 'DELETE' }),
    onSuccess: (_result, input) => {
      qc.removeQueries({ queryKey: qk.device(input.deviceId) });
      void qc.invalidateQueries({ queryKey: qk.devices(input.homeId) });
      qc.invalidateQueries({ queryKey: qk.dashboard(input.homeId) });
    },
  });
}

/**
 * Move a device into a room, or out of one (`roomId: null`).
 *
 * The API distinguishes "field absent" from "field null" so clearing a room is
 * expressible; this sends the field explicitly, always.
 */
export function useSetRoom() {
  const qc = useQueryClient();

  return useMutation({
    mutationFn: (input: { deviceId: string; roomId: string | null }) =>
      request<Device>(`/devices/${input.deviceId}`, {
        method: 'PATCH',
        body: JSON.stringify({ room_id: input.roomId }),
      }),
    onSuccess: (device) => {
      qc.setQueryData(qk.device(device.id), device);
      void qc.invalidateQueries({ queryKey: qk.devices(device.home_id ?? 'none') });
    },
  });
}
