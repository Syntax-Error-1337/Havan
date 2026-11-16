import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { request } from '../../shared/api/client';
import { qk } from '../../shared/api/keys';

export type Role = 'owner' | 'admin' | 'developer' | 'viewer';

export type TeamMember = {
  user_id: string;
  email: string;
  display_name: string;
  role: Role;
  last_login_at: string | null;
};

export type PendingInvite = {
  id: string;
  email: string;
  role: Role;
  expires_at: string;
  expired: boolean;
};

export type Team = {
  members: TeamMember[];
  invites: PendingInvite[];
};

export function useTeam() {
  return useQuery({ queryKey: qk.team, queryFn: () => request<Team>('/vendor/team') });
}

/**
 * The token comes back once and is held in the mutation result, not the query cache: a cached
 * invitation would sit in memory for every mounted screen and survive a tab switch.
 */
export function useInvite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: { email: string; role: Exclude<Role, 'owner'> }) =>
      request<{ token: string; note: string }>('/vendor/team', {
        method: 'POST',
        body: JSON.stringify(body),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.team }),
  });
}

export function useRemoveMember() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      request<{ removed: string }>(`/vendor/team/members/${id}`, { method: 'DELETE' }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.team }),
  });
}

/** Which roles a given actor may hand out — mirrors the server rule, for the form only. */
export function invitableRoles(actor: Role): Array<Exclude<Role, 'owner'>> {
  if (actor === 'owner') return ['admin', 'developer', 'viewer'];
  if (actor === 'admin') return ['developer', 'viewer'];
  return [];
}
