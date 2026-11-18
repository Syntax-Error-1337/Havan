import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { request } from '../../shared/api/client';
import { qk } from '../../shared/api/keys';

export type ApiKey = {
  id: string;
  name: string;
  prefix: string;
  /** Present only on the response that created the key. */
  secret?: string;
  created_at: string;
  last_used_at: string | null;
  revoked_at: string | null;
};

export function useApiKeys() {
  return useQuery({
    queryKey: qk.apiKeys,
    queryFn: () => request<ApiKey[]>('/vendor/api-keys'),
  });
}

/**
 * The secret comes back once, so it is held in the mutation result rather than
 * put in the query cache: a cache entry would keep it in memory for every
 * mounted screen and survive a tab switch.
 */
export function useCreateApiKey() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { name: string }) =>
      request<ApiKey>('/vendor/api-keys', {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.apiKeys }),
  });
}

export function useRevokeApiKey() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => request<void>(`/vendor/api-keys/${id}`, { method: 'DELETE' }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.apiKeys }),
  });
}

/** A key that never says otherwise is live; say so plainly in the table. */
/**
 * The lifecycle state of a key: `active` until it is revoked, and nothing else.
 *
 * It used to return `unused` for a key nobody had called yet, which put a *usage* fact in the *state* column and left the
 * reader to work out whether an unused key still worked. Whether it has been used is the column beside it.
 */
export function keyState(key: ApiKey): 'active' | 'revoked' {
  return key.revoked_at ? 'revoked' : 'active';
}
