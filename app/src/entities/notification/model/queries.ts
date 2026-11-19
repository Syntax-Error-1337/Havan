import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { POLL_MS } from '../../../shared/config/polling';

import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';

export type Notification = {
  id: string;
  kind: string;
  title: string;
  body: string;
  data: Record<string, unknown>;
  created_at: string;
  read_at: string | null;
};

export function useNotifications(limit = 50) {
  return useQuery({
    queryKey: qk.notifications,
    queryFn: () => request<Notification[]>(`/notifications?limit=${limit}`),
    // Alerts arrive from automations that may fire while the screen is open.
    refetchInterval: POLL_MS,
  });
}

/**
 * Which phones will buzz for this account.
 *
 * Registration existed since the push sender landed and nothing could read it back, so Settings could not answer the
 * question people actually ask — "is this phone going to notify me?" — which they usually ask because it is not.
 */
export function usePushTokens() {
  return useQuery({
    queryKey: qk.pushTokens,
    queryFn: () => request<{ count: number; devices: { platform: string; app_version: string | null }[] }>('/notifications/tokens'),
    // Push registration changes when somebody installs or uninstalls the app, not on a timer.
    staleTime: 5 * 60_000,
  });
}

export type QuietHours = { from: string | null; to: string | null; timezone: string | null };

/**
 * The window in which pushes are held.
 *
 * Read back from the server rather than kept in the app: the server is the thing that enforces it, and a setting whose
 * position depends on local state eventually disagrees with what happens at 03:00.
 */
export function useQuietHours() {
  return useQuery({
    queryKey: qk.quietHours,
    queryFn: () => request<QuietHours>('/notifications/quiet-hours'),
    staleTime: 5 * 60_000,
  });
}

/**
 * Set or clear the window.
 *
 * The server refuses a time it cannot parse, a window with no width, and a zone Postgres does not know — all three would
 * otherwise look like a setting and behave like nothing.
 */
export function useSetQuietHours() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (hours: Partial<QuietHours>) =>
      request<QuietHours>('/notifications/quiet-hours', {
        method: 'PUT',
        body: JSON.stringify(hours),
      }),
    onSuccess: (saved) => {
      qc.setQueryData(qk.quietHours, saved);
    },
  });
}

/**
 * Turn alerts off on every phone.
 *
 * Deliberately not a toggle: a toggle implies a state this app cannot read back per device, and a control whose position
 * depends on a guess is worse than a button that says what it did. The confirmation says the honest part — alerts still
 * appear in the app.
 */
export function useTurnOffAlerts() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => request<{ turned_off: number }>('/notifications/tokens', { method: 'DELETE' }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.pushTokens });
    },
  });
}

/**
 * Throw one alert away.
 *
 * The feed could only be marked read, never shortened: an alert list that only grows is a list people stop opening.
 * The endpoint is `DELETE /notifications/{id}` and it answers `removed: false` for an id that is already gone, which
 * is a second phone — not an error.
 */
export function useDismissAlert() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      request<{ removed: boolean }>(`/notifications/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.notifications });
      void qc.invalidateQueries({ queryKey: qk.unread });
    },
  });
}

export function useUnreadCount() {
  return useQuery({
    queryKey: qk.unread,
    queryFn: () => request<{ unread: number }>('/notifications/unread-count'),
    refetchInterval: POLL_MS,
  });
}

export function useMarkRead() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      request<{ changed: boolean }>(`/notifications/${id}/read`, { method: 'POST' }),
    // Optimistic: tapping an alert and watching it stay bold for a round trip
    // reads as "it did not work".
    onMutate: async (id) => {
      await qc.cancelQueries({ queryKey: qk.notifications });
      const previous = qc.getQueryData<Notification[]>(qk.notifications);
      qc.setQueryData<Notification[]>(qk.notifications, (list) =>
        (list ?? []).map((n) =>
          n.id === id ? { ...n, read_at: n.read_at ?? new Date().toISOString() } : n,
        ),
      );
      return { previous };
    },
    onError: (_e, _id, context) => {
      if (context?.previous) qc.setQueryData(qk.notifications, context.previous);
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: qk.notifications });
      void qc.invalidateQueries({ queryKey: qk.unread });
    },
  });
}

export function useMarkAllRead() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => request<{ cleared: number }>('/notifications/read-all', { method: 'POST' }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.notifications });
      void qc.invalidateQueries({ queryKey: qk.unread });
    },
  });
}

/**
 * "just now", "12m ago", "3h ago", then what a person actually says: "Yesterday, 4:05 PM", "Tue, 9 AM",
 * and only after a week a date.
 *
 * The old version printed `3 h ago` (a space that made every row one character wider than it needed) and
 * then fell off a cliff to `11/16/2026` — a format that is ambiguous across locales and unreadable at a
 * glance in a list you are scanning.
 */
export function ago(iso: string, now = Date.now()): string {
  const then = new Date(iso);
  const secs = Math.max(0, Math.round((now - then.getTime()) / 1000));

  if (secs < 60) return 'just now';
  if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
  if (secs < 86_400) return `${Math.floor(secs / 3600)}h ago`;

  const time = then.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  const days = Math.floor(secs / 86_400);
  if (days === 1) return `Yesterday, ${time}`;
  if (days < 7) return `${then.toLocaleDateString([], { weekday: 'short' })}, ${time}`;
  return then.toLocaleDateString([], { day: 'numeric', month: 'short' });
}

/**
 * The time of day, for a row that already sits under a day heading.
 *
 * A grouped feed that repeats "10h ago" under "YESTERDAY" is telling the same fact twice in two scales —
 * the person can see which day it was, so what they need is when in that day.
 */
export function clockTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  return d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
}

/**
 * The heading a feed groups under: `Today`, `Yesterday`, then the date.
 *
 * A flat list of eleven alerts from four days is a list you scroll without reading. Grouping costs one
 * line of code and turns it into a day-by-day record you can navigate.
 */
export function dayBucket(iso: string, now = Date.now()): string {
  const then = new Date(iso);
  const day = 86_400_000;
  const startOfToday = new Date(now).setHours(0, 0, 0, 0);
  const at = then.getTime();
  if (at >= startOfToday) return 'Today';
  if (at >= startOfToday - day) return 'Yesterday';
  return then.toLocaleDateString([], { weekday: 'long', day: 'numeric', month: 'long' });
}
