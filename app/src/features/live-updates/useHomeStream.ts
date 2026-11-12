import { useEffect } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { BASE } from '../../shared/api/client';
import { qk } from '../../shared/api/keys';
import { useSession } from '../../entities/session';
import type { Device } from '../../entities/device/model/types';
import type { Dashboard } from '../../entities/home/model/dashboard';

/**
 * One socket per signed-in app, one subscription per home.
 *
 * Everything the server pushes lands in the query cache, so a screen that is
 * already open repaints without a refetch, and a screen that is closed is
 * correct the moment it mounts (it reads the same cache).
 */
export function useHomeStream(homeId: string | null) {
  const qc = useQueryClient();
  const access = useSession((s) => s.access);

  useEffect(() => {
    if (!homeId || !access) return;

    // http://host:8080/v1 → ws://host:8080/v1
    const url = `${BASE.replace(/^http/, 'ws')}/stream`;
    const socket = new WebSocket(url);

    socket.onopen = () => {
      socket.send(JSON.stringify({ t: 'auth', token: access, homes: [homeId] }));
    };

    socket.onmessage = (event) => {
      const msg = safeParse(String(event.data));
      if (!msg || !msg.device_id) return;
      const deviceId = msg.device_id;

      // The dashboard holds its own copy of the device list; a screen that reads it
      // must repaint from the same event, not from the next refetch.
      const patchDashboard = (patch: (d: Device) => Device) =>
        qc.setQueryData<Dashboard>(qk.dashboard(homeId), (dash) =>
          dash ? { ...dash, devices: dash.devices.map(patch) } : dash,
        );

      switch (msg.t) {
        case 'point.changed':
          patchDashboard((d) =>
            d.id === deviceId
              ? {
                  ...d,
                  points: d.points.map((p) =>
                    p.code === msg.code ? { ...p, value: msg.value as never } : p,
                  ),
                }
              : d,
          );
          qc.setQueryData<Device[]>(qk.devices(homeId), (devices) =>
            devices?.map((d) =>
              d.id === deviceId
                ? {
                    ...d,
                    points: d.points.map((p) =>
                      p.code === msg.code ? { ...p, value: msg.value as never } : p,
                    ),
                  }
                : d,
            ),
          );
          // The detail screen holds its own key; keep both in step.
          qc.setQueryData<Device>(qk.device(deviceId), (d) =>
            d
              ? {
                  ...d,
                  points: d.points.map((p) =>
                    p.code === msg.code ? { ...p, value: msg.value as never } : p,
                  ),
                }
              : d,
          );
          break;

        case 'device.online':
        case 'device.offline': {
          const online = msg.t === 'device.online';
          const patch = (d: Device) => (d.id === deviceId ? { ...d, online } : d);
          patchDashboard(patch);
          qc.setQueryData<Device[]>(qk.devices(homeId), (devices) => devices?.map(patch));
          qc.setQueryData<Device>(qk.device(deviceId), (d) => (d ? patch(d) : d));
          break;
        }

        case 'command.settled':
          // The settle is authoritative; the optimistic value was a guess.
          qc.invalidateQueries({ queryKey: qk.device(deviceId) });
          break;

        default:
          break;
      }
    };

    return () => socket.close();
  }, [homeId, access, qc]);
}

type Incoming = {
  t: string;
  device_id?: string;
  code?: string;
  value?: unknown;
};

function safeParse(raw: string): Incoming | null {
  try {
    return JSON.parse(raw) as Incoming;
  } catch {
    return null;
  }
}
