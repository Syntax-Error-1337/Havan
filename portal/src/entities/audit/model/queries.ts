import { useInfiniteQuery } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';

export type AuditEntry = {
  id: string;
  ts: string;
  actor_kind: 'user' | 'api_key' | 'system';
  actor_id: string | null;
  /** The actor's address, resolved at read time. Null for a key, for automation, or for a member since removed. */
  actor_email: string | null;
  action: string;
  target_kind: string;
  target_id: string | null;
  request_id: string | null;
  ip: string | null;
  detail: Record<string, unknown>;
};

export type AuditPage = {
  entries: AuditEntry[];
  next_cursor: string | null;
};

/** The vendor's own trail, newest first, a page at a time. Read-only by design. */
export type AuditFilters = {
  /** Exact action, from the closed set the trail actually contains. */
  action?: string;
  /** Part of an actor's address. */
  actor?: string;
};

/**
 * The trail, a page at a time, optionally narrowed.
 *
 * The filters travel with the cursor: page two of "webhook.created" is the second page of that action, not the second
 * page of everything. They are part of the query key for the same reason — a filter that does not restart the list
 * would append today's answers to yesterday's question.
 */
export function useAudit(filters: AuditFilters = {}) {
  const params = new URLSearchParams();
  if (filters.action) params.set('action', filters.action);
  if (filters.actor) params.set('actor', filters.actor);
  const narrowed = params.toString();

  return useInfiniteQuery({
    queryKey: ['audit', narrowed],
    queryFn: ({ pageParam }) => {
      const query = new URLSearchParams(narrowed);
      if (pageParam) query.set('cursor', pageParam);
      const qs = query.toString();
      return request<AuditPage>(`/vendor/audit${qs ? `?${qs}` : ''}`);
    },
    initialPageParam: null as string | null,
    getNextPageParam: (last: AuditPage) => last.next_cursor,
  });
}
