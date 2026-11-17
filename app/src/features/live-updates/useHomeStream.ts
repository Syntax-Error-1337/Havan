import { useEffect, useRef, useState } from 'react';
import { AppState } from 'react-native';
import { useQueryClient } from '@tanstack/react-query';

import { BASE } from '../../shared/api/client';
import { qk } from '../../shared/api/keys';
import { useSession } from '../../entities/session';
import type { Device } from '../../entities/device/model/types';
import type { Dashboard } from '../../entities/home/model/dashboard';

/**
 * How the phone stands with the platform's live stream.
 *
 * `live` is the only state in which the screen can be trusted without a refetch; `reconnecting` is
 * said to the customer rather than hidden, because a dashboard that silently freezes on yesterday's
 * temperature is worse than one that admits it is catching up.
 */
export type StreamStatus = 'connecting' | 'live' | 'reconnecting';

/** First retry after 500 ms, doubling to a ceiling of 30 s. */
const RETRY_BASE_MS = 500;
const RETRY_CEILING_MS = 30_000;

/**
 * How long to wait before attempt `attempt` (0-based).
 *
 * Exported and pure because it is the only part of the reconnect that can be checked without a clock:
 * the doubling, and the ceiling that keeps a long outage from turning into a silent hour-long wait.
 * The caller adds up to 250 ms of jitter on top.
 */
export function retryDelayMs(attempt: number): number {
  return Math.min(RETRY_CEILING_MS, RETRY_BASE_MS * 2 ** Math.max(0, attempt));
}

/**
 * One socket per signed-in app, one subscription per home — and it survives the real world.
 *
 * Everything the server pushes lands in the query cache, so a screen that is already open repaints
 * without a refetch, and a screen that is closed is correct the moment it mounts (it reads the same
 * cache).
 *
 * A socket opened once and never reopened works in a demo and dies on a train. Phones suspend
 * connections when the app is backgrounded and change networks between the front door and the car, so
 * this reconnects with a jittered backoff, treats every reconnection as "events were missed" and
 * refetches what it holds, and reconnects *immediately* when the app returns to the foreground
 * instead of waiting out a timer set twenty minutes earlier.
 */
export function useHomeStream(homeId: string | null): StreamStatus {
  const qc = useQueryClient();
  const access = useSession((s) => s.access);
  const [status, setStatus] = useState<StreamStatus>('connecting');

  // The socket has to outlive renders, and the resume listener needs to reach it without the effect
  // re-running (which would close a healthy connection on every session update).
  const socketRef = useRef<WebSocket | null>(null);
  const disposed = useRef(false);

  useEffect(() => {
    if (!homeId || !access) return;

    // http://host:8080/v1 → ws://host:8080/v1
    const url = `${BASE.replace(/^http/, 'ws')}/stream`;
    let attempt = 0;
    let retry: ReturnType<typeof setTimeout> | null = null;
    let missedEvents = false;
    disposed.current = false;

    const onMessage = (event: { data: string }) => {
      const msg = safeParse(String(event.data));
      if (!msg) return;

      // An alert is about a person, not a device, and it arrives on `user:{id}`. The count is bumped
      // in place — this frame *is* the new alert — and the list is invalidated so opening /alerts
      // shows it without waiting for the safety-net poll.
      if (msg.t === 'notification') {
        patchDashboards((dash) => ({ ...dash, unread_count: dash.unread_count + 1 }));
        qc.setQueryData<{ unread: number }>(qk.unread, (count) => ({
          unread: (count?.unread ?? 0) + 1,
        }));
        qc.invalidateQueries({ queryKey: qk.notifications });
        return;
      }

      if (!msg.device_id) return;
      const deviceId = msg.device_id;

      // The dashboard holds its own copy of the device list, and it keeps one cache entry *per page
      // size* — `['home', id, 'dashboard', 200]`, `…, 420]`. Writing a single key meant every live
      // event landed in an entry no screen reads, which is why the dashboard never moved in the
      // running app while the tests (which seeded that same key) passed. `setQueriesData` with the
      // prefix patches every page size that exists.
      const patchDashboard = (patch: (d: Device) => Device) =>
        patchDashboards((dash) => ({ ...dash, devices: dash.devices.map(patch) }));

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

    /** Patch every cached dashboard for this home, whatever page size it was fetched at. */
    const patchDashboards = (patch: (dash: Dashboard) => Dashboard) =>
      qc.setQueriesData<Dashboard>({ queryKey: qk.dashboard(homeId) }, (dash) =>
        dash ? patch(dash) : dash,
      );

    /** What a missed window costs: the home may have changed while we were not listening. */
    const resync = () => {
      qc.invalidateQueries({ queryKey: qk.dashboard(homeId) });
      qc.invalidateQueries({ queryKey: qk.devices(homeId) });
    };

    /** Close what we have without letting its close event start a retry chain. */
    const drop = () => {
      const socket = socketRef.current;
      socketRef.current = null;
      if (!socket) return;
      socket.onopen = null;
      socket.onmessage = null;
      socket.onclose = null;
      socket.onerror = null;
      try {
        socket.close();
      } catch {
        // Already-closed sockets throw on some platforms; there is nothing useful to do about it.
      }
    };

    const connect = () => {
      if (disposed.current) return;
      drop();
      setStatus(attempt === 0 ? 'connecting' : 'reconnecting');

      const socket = new WebSocket(url);
      socketRef.current = socket;

      socket.onopen = () => {
        attempt = 0;
        setStatus('live');
        socket.send(JSON.stringify({ t: 'auth', token: access, homes: [homeId] }));
        if (missedEvents) {
          missedEvents = false;
          resync();
        }
      };

      socket.onmessage = onMessage;

      socket.onerror = () => {
        // `onclose` follows on every platform that matters; retrying from here would double up.
      };

      socket.onclose = () => {
        // A socket we replaced or closed ourselves must not schedule anything.
        if (disposed.current || socketRef.current !== socket) return;
        socketRef.current = null;
        missedEvents = true;
        setStatus('reconnecting');
        const backoff = retryDelayMs(attempt);
        attempt += 1;
        // Jitter, so a hundred thousand phones that lost the same cell tower do not come back in step.
        retry = setTimeout(connect, backoff + Math.random() * 250);
      };
    };

    connect();

    // A backgrounded phone is not a disconnected customer: coming back should not wait for a timer
    // that was set before the screen was turned off.
    const subscription = AppState.addEventListener('change', (next) => {
      if (next !== 'active') return;
      const socket = socketRef.current;
      // CONNECTING (0) and OPEN (1) are both fine; anything else is a corpse that has not noticed.
      if (socket && (socket.readyState === 0 || socket.readyState === 1)) return;
      if (retry) clearTimeout(retry);
      retry = null;
      attempt = 0;
      connect();
    });

    return () => {
      disposed.current = true;
      if (retry) clearTimeout(retry);
      subscription.remove();
      drop();
    };
  }, [homeId, access, qc]);

  return status;
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
