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
export function useAudit() {
  return useInfiniteQuery({
    queryKey: ['audit'],
    queryFn: ({ pageParam }) =>
      request<AuditPage>(`/vendor/audit${pageParam ? `?cursor=${pageParam}` : ''}`),
    initialPageParam: null as string | null,
    getNextPageParam: (last: AuditPage) => last.next_cursor,
  });
}
