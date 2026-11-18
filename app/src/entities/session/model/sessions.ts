import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';
import { useSession } from './store';

/** One sign-in, as the API describes it. No device names — the service never stored any. */
export type Session = {
  family_id: string;
  started_at: string;
  last_seen_at: string;
  expires_at: string;
};

/**
 * The devices this account is signed in on.
 *
 * The table behind this has carried a family id and a revocation timestamp since the first migration, and nothing in the
 * app could see it: a customer who left a session open on a phone they sold had no way to end it from anywhere they
 * could still sign in. This is the read; `useSignOutOthers` is the write.
 */
export function useSessions() {
  const access = useSession((s) => s.access);
  return useQuery({
    queryKey: qk.sessions,
    queryFn: () => request<{ count: number; sessions: Session[] }>('/me/sessions'),
    enabled: Boolean(access),
    // Sessions change when somebody signs in on another device, not on a timer; a minute of staleness is honest.
    staleTime: 60_000,
  });
}

/**
 * Change the password, with the old one as proof.
 *
 * The request carries this device's refresh token so the server can keep this session and end the others: signing the
 * person out of the phone they just fixed their password on is a punishment for doing the right thing.
 */
export function useChangePassword() {
  const qc = useQueryClient();
  const refresh = useSession((s) => s.refresh);

  return useMutation({
    mutationFn: (input: { current: string; next: string }) =>
      request<{ signed_out: number }>('/auth/password', {
        method: 'POST',
        body: JSON.stringify({ ...input, refresh }),
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.sessions });
    },
  });
}

/**
 * Sign out everywhere except this device.
 *
 * The refresh token names "this device" — it carries the family id — so the caller never sends an id it could guess. A
 * malformed token revokes nothing rather than everything: a typo must not end sessions the customer is still using, and
 * the server enforces that (see `api::routes::auth::logout_others`).
 */
export function useSignOutOthers() {
  const qc = useQueryClient();
  const refresh = useSession((s) => s.refresh);

  return useMutation({
    mutationFn: () =>
      request<{ revoked: number }>('/auth/logout-others', {
        method: 'POST',
        body: JSON.stringify({ refresh }),
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.sessions });
    },
  });
}
