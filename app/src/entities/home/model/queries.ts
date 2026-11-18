import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';
import type { Home, Role } from './types';

/**
 * The account's homes.
 *
 * `enabled` exists because the dashboard already returns the home it is showing: with
 * a stored home id this call is a round trip that changes nothing. It still runs when
 * the app has to decide *which* home to open — the first launch on a new account.
 */
export function useHomes(onlyWithoutHome = false) {
  return useQuery({
    queryKey: qk.homes,
    queryFn: () => request<Home[]>('/homes'),
    enabled: !onlyWithoutHome,
    // Homes change rarely; refetching on every screen focus is noise.
    staleTime: 60_000,
  });
}

/**
 * Move the home to another timezone.
 *
 * Every schedule in the home is read against this, so it is one query with one cache invalidation —
 * a home screen still showing yesterday's zone is a schedule that looks right and fires wrong.
 */
export function useSetTimeZone() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { id: string; timezone: string }) =>
      request<void>(`/homes/${input.id}`, {
        method: 'PATCH',
        body: JSON.stringify({ timezone: input.timezone }),
      }),
    onSuccess: (_data, input) => {
      void qc.invalidateQueries({ queryKey: qk.homes });
      void qc.invalidateQueries({ queryKey: qk.dashboard(input.id) });
    },
  });
}

/**
 /** A person in the home, as the API returns them. */
 export type Member = {
   user_id: string;
   display_name: string;
   email?: string | null;
   role: Role;
   /** Whether they may pair a device without asking an admin. */
   can_pair: boolean;
 };

 /**
  * Who is in the home.
  *
  * The API has served this list, invitations, role changes and removals since the first version, and the app showed
  * one number — "2 members" — and a way to change neither. For a smart-home app that is the family feature.
  */
 export function useMembers(homeId: string | undefined) {
   return useQuery({
     queryKey: qk.members(homeId ?? 'none'),
     queryFn: () => request<Member[]>(`/homes/${homeId}/members`),
     enabled: Boolean(homeId),
   });
 }

 /**
  * Add somebody who already has an account.
  *
  * The server looks the email up and answers 404 "account with that email" if it is not one of ours — an invitation
  * to an address in this platform is a membership, not an email, and there is no mail transport to send one. The
  * screen says so rather than promising a letter.
  */
 /**
 * What inviting somebody did.
 *
 * `added` means the address already had an account and is in the home now. `invited` means it did not, and carries the
 * token that is the only way for them to get in — shown once, because there is still no mail transport.
 */
export type InviteResult =
  | { outcome: 'added' }
  | { outcome: 'invited'; token: string; expires_at: string };

export function useInviteMember() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { homeId: string; email: string; role: Role }) =>
      request<InviteResult>(`/homes/${input.homeId}/members`, {
        method: 'POST',
        body: JSON.stringify({ email: input.email, role: input.role }),
      }),
    onSuccess: (_data, input) => {
      void qc.invalidateQueries({ queryKey: qk.members(input.homeId) });
      void qc.invalidateQueries({ queryKey: qk.invitations(input.homeId) });
      // The count on the homes list and on Settings comes from the home row.
      void qc.invalidateQueries({ queryKey: qk.homes });
    },
  });
}

/** The invitation waiting to be accepted for a home. */
export type PendingInvite = {
  id: string;
  home_id: string;
  email: string;
  role: Role;
  created_at: string;
  expires_at: string;
};

export function useInvitations(homeId: string | undefined) {
  return useQuery({
    queryKey: qk.invitations(homeId ?? 'none'),
    queryFn: () => request<PendingInvite[]>(`/homes/${homeId}/invitations`),
    enabled: Boolean(homeId),
    staleTime: 30_000,
  });
}

/**
 * Accept an invitation. The home comes back, so the caller can open it.
 *
 * The address has to match the one the invitation was sent to — the server refuses otherwise, because a forwarded link
 * is a forward, not a key.
 */
export function useAcceptInvite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (token: string) =>
      request<{ home_id: string }>('/invitations/accept', {
        method: 'POST',
        body: JSON.stringify({ token }),
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.homes });
    },
  });
}

export function useRevokeInvitation(homeId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      request<void>(`/homes/${homeId}/invitations/${id}`, { method: 'DELETE' }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: qk.invitations(homeId ?? 'none') });
    },
  });
}

 export function useSetMemberRole() {
   const qc = useQueryClient();
   return useMutation({
     mutationFn: (input: { homeId: string; userId: string; role: Role }) =>
       request<void>(`/homes/${input.homeId}/members/${input.userId}`, {
         method: 'PATCH',
         body: JSON.stringify({ role: input.role }),
       }),
     onSuccess: (_data, input) => {
       void qc.invalidateQueries({ queryKey: qk.members(input.homeId) });
     },
   });
 }

 export function useRemoveMember() {
   const qc = useQueryClient();
   return useMutation({
     mutationFn: (input: { homeId: string; userId: string }) =>
       request<void>(`/homes/${input.homeId}/members/${input.userId}`, { method: 'DELETE' }),
     onSuccess: (_data, input) => {
       void qc.invalidateQueries({ queryKey: qk.members(input.homeId) });
       void qc.invalidateQueries({ queryKey: qk.homes });
     },
   });
 }

 /** Where the home is, for the rules that talk about sunrise and sunset.
 *
 * `null` clears it: both coordinates are sent as null together, because the service refuses a location with only
 * one of them — and a customer who moves house should be able to say "not any more".
 */
export function useSetLocation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { id: string; lat: number | null; lon: number | null }) =>
      request<void>(`/homes/${input.id}`, {
        method: 'PATCH',
        body: JSON.stringify({ geo_lat: input.lat, geo_lon: input.lon }),
      }),
    onSuccess: (_data, input) => {
      void qc.invalidateQueries({ queryKey: qk.homes });
      void qc.invalidateQueries({ queryKey: qk.dashboard(input.id) });
    },
  });
}

export function useRenameHome() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { id: string; name: string }) =>
      request<void>(`/homes/${input.id}`, {
        method: 'PATCH',
        body: JSON.stringify({ name: input.name }),
      }),
    onSuccess: (_data, input) => {
      void qc.invalidateQueries({ queryKey: qk.homes });
      void qc.invalidateQueries({ queryKey: qk.dashboard(input.id) });
    },
  });
}

/**
 * Delete a home.
 *
 * The server has served `DELETE /homes/{id}` since the first version and nothing in the app called it, so a home made
 * by mistake — or the second home somebody made to see what would happen — could not be removed from inside the
 * product at all. Devices, rooms, scenes and rules go with it, which is why every caller confirms first.
 */
export function useDeleteHome() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => request<void>(`/homes/${id}`, { method: 'DELETE' }),
    onSuccess: (_data, id) => {
      void qc.invalidateQueries({ queryKey: qk.homes });
      void qc.invalidateQueries({ queryKey: qk.dashboard(id) });
    },
  });
}

export function useCreateHome() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { name: string; timezone: string }) =>
      request<Home>('/homes', { method: 'POST', body: JSON.stringify(input) }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.homes }),
  });
}
