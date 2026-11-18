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
 * One point, moved, in a device that is already on screen.
 *
 * A pure function so the optimistic write can be asserted without a router: it is the same shape for the device
 * cache and for the list the dashboard draws its tiles from, and the two drifted apart — see `useSetPoint`.
 */
export function withPointValue(
  device: Device,
  code: string,
  value: boolean | number | string,
): Device {
  return {
    ...device,
    points: device.points.map((p) => (p.code === code ? { ...p, value } : p)),
  };
}

/**
 * The same optimistic write, into whatever a home cache happens to hold.
 *
 * Two shapes live under `['home', …]`: the device list is a `Device[]`, and the dashboard aggregate is an object
 * with a `devices` array. The screen the customer toggles from reads the *aggregate*, so a patch that only knew
 * about arrays changed a cache nobody was drawing. Returns the value unchanged when the shape is unknown, so the
 * caller can skip the write.
 */
export function patchDeviceIn(
  data: unknown,
  deviceId: string,
  code: string,
  value: boolean | number | string,
): unknown {
  const update = (device: Device) =>
    device.id === deviceId ? withPointValue(device, code, value) : device;

  if (Array.isArray(data)) {
    const list = data as Device[];
    if (!list.some((d) => d?.id === deviceId)) return data;
    return list.map(update);
  }
  if (data && typeof data === 'object' && Array.isArray((data as { devices?: unknown }).devices)) {
    const holder = data as { devices: Device[] };
    if (!holder.devices.some((d) => d?.id === deviceId)) return data;
    return { ...holder, devices: holder.devices.map(update) };
  }
  return data;
}

/**
 * Optimistic on purpose: a toggle that waits for the cloud feels broken. The
 * command may still settle `rejected`, and then we roll back with the device's
 * own message.
 *
 * The write also lands in the **device lists** — `['home', homeId, 'devices']` — not only in the single-device
 * cache. The dashboard's tiles read from the list, so before this the switch under a customer's thumb did not
 * move until the socket echoed back: the toggle looked broken on the one screen it is used most, and only
 * looked right on the detail screen that hardly anybody opens.
 */
export function useSetPoint(deviceId: string) {
  const qc = useQueryClient();
  /**
   * Every cache under `['home', …]` that holds devices: the plain list (`…/devices`) *and* the dashboard
   * aggregate (`…/dashboard`), which is what the tiles are actually drawn from. Patching only the list is why
   * the optimistic write had no visible effect.
   */
  const homeKey = { queryKey: ['home'] as const };

  return useMutation({
    mutationFn: (input: { code: string; value: boolean | number | string }) =>
      request<{ command_id: string }>(`/devices/${deviceId}/commands`, {
        method: 'POST',
        body: JSON.stringify({ code: input.code, value: input.value }),
      }),
    onMutate: async (input) => {
      await qc.cancelQueries({ queryKey: qk.device(deviceId) });
      await qc.cancelQueries(homeKey);

      const previous = qc.getQueryData<Device>(qk.device(deviceId));
      if (previous) {
        qc.setQueryData<Device>(qk.device(deviceId), withPointValue(previous, input.code, input.value));
      }

      // Every home-scoped cache holding this device, whatever its shape.
      const previousHomes = qc.getQueriesData(homeKey);
      for (const [key, data] of previousHomes) {
        const patched = patchDeviceIn(data, deviceId, input.code, input.value);
        if (patched !== data) qc.setQueryData(key, patched);
      }

      return { previous, previousHomes };
    },
    onError: (_err, _input, ctx) => {
      if (ctx?.previous) qc.setQueryData(qk.device(deviceId), ctx.previous);
      for (const [key, data] of ctx?.previousHomes ?? []) qc.setQueryData(key, data);
    },
    onSettled: () => {
      // The websocket settles it; a refetch here covers the socket-missed case.
      qc.invalidateQueries({ queryKey: qk.device(deviceId) });
      // The home screen shows the same points; without this it keeps a value the
      // device has already moved past (no socket, e.g. when offline).
      void qc.invalidateQueries(homeKey);
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
