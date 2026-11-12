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

/** "just now", "12 min ago", "3 h ago", then a date. */
export function ago(iso: string, now = Date.now()): string {
  const then = new Date(iso).getTime();
  const secs = Math.max(0, Math.round((now - then) / 1000));

  if (secs < 60) return 'just now';
  if (secs < 3600) return `${Math.floor(secs / 60)} min ago`;
  if (secs < 86_400) return `${Math.floor(secs / 3600)} h ago`;
  return new Date(iso).toLocaleDateString();
}
