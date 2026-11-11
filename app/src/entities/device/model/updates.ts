import { useQuery } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';

export type DeviceUpdate = {
  current: string | null;
  target: string | null;
  state: string | null;
  error: string | null;
  at: string | null;
  in_progress: boolean;
};

export function useDeviceUpdate(deviceId: string | undefined) {
  return useQuery({
    queryKey: qk.deviceUpdate(deviceId ?? 'none'),
    queryFn: () => request<DeviceUpdate>(`/devices/${deviceId}/updates`),
    enabled: Boolean(deviceId),
    // A firmware job moves over minutes, not seconds, and the socket carries the
    // device's own state. This is a status line, not a live feed.
    staleTime: 30_000,
  });
}

/**
 * One sentence a customer can act on.
 *
 * "Update failed" on its own is a support call. The reason is what turns it into
 * either "wait" or "call us", and the version is what makes the sentence concrete.
 */
export function describeUpdate(u: DeviceUpdate | undefined): { text: string; bad: boolean } | null {
  if (!u || !u.state) return null;

  const to = u.target ?? 'a newer version';

  if (u.in_progress) {
    return { text: `Updating to ${to}…`, bad: false };
  }
  if (u.state === 'success') {
    return { text: `Up to date (${u.current ?? to})`, bad: false };
  }
  if (u.state === 'failed') {
    return {
      text: `Update to ${to} failed${u.error ? `: ${u.error}` : ''}`,
      bad: true,
    };
  }
  if (u.state === 'skipped') {
    return { text: `Update to ${to} was skipped`, bad: false };
  }
  return { text: `Update to ${to}: ${u.state}`, bad: false };
}
