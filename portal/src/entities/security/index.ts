import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { request } from '../../shared/api/client';
import { qk } from '../../shared/api/keys';

export type SecurityPolicy = {
  totp_required: boolean;
  members: number;
  /** Members who have proved they can generate a code. */
  enrolled: number;
};

export type Enrolment = {
  secret: string;
  otpauth_uri: string;
};

export type SsoConfig = {
  required: boolean;
  domain: string | null;
  bound_members: number;
  total_members: number;
  enforced: boolean;
};

export function useSecurity() {
  return useQuery({
    queryKey: qk.security,
    queryFn: () => request<SecurityPolicy>('/vendor/security'),
  });
}

export function useSso() {
  return useQuery({
    queryKey: [...qk.security, 'sso'] as const,
    queryFn: () => request<SsoConfig>('/vendor/sso'),
  });
}

export function useSetTotpPolicy() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (required: boolean) =>
      request<SecurityPolicy>('/vendor/security', {
        method: 'PATCH',
        body: JSON.stringify({ totp_required: required }),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.security }),
  });
}

/**
 * The secret and its `otpauth://` URI, in the mutation result rather than the cache: it is a
 * credential, and a cache entry keeps it for every mounted screen and across a tab switch.
 */
export function useEnrolTotp() {
  return useMutation({
    mutationFn: () => request<Enrolment>('/vendor/totp/enrol', { method: 'POST' }),
  });
}

export function useConfirmTotp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (code: string) =>
      request<SecurityPolicy>('/vendor/totp/confirm', {
        method: 'POST',
        body: JSON.stringify({ code }),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.security }),
  });
}

export function useDisableTotp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => request<SecurityPolicy>('/vendor/totp', { method: 'DELETE' }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.security }),
  });
}

export function useSetSso() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { required: boolean; domain: string }) =>
      request<SsoConfig>('/vendor/sso', {
        method: 'PATCH',
        body: JSON.stringify(input),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: [...qk.security, 'sso'] }),
  });
}

/**
 * Bind (or clear) a member's identity-provider subject.
 *
 * The route existed from the first day of SSO and no screen called it, so `enforced` could never
 * become true: the guard refuses enforcement while anybody is unbound, and nobody could bind. An
 * empty subject is the unbind — which the server refuses while SSO is required.
 */
export function useBindSso() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { userId: string; subject: string }) =>
      request<{ bound: string }>(`/vendor/team/${input.userId}/sso`, {
        method: 'POST',
        body: JSON.stringify({ subject: input.subject }),
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: qk.team });
      qc.invalidateQueries({ queryKey: [...qk.security, 'sso'] });
    },
  });
}
