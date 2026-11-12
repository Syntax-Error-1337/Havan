import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';
import type { Campaign, CampaignRow, Firmware, Rollback } from './types';

export function useFirmware(productId: string | undefined) {
  return useQuery({
    queryKey: ['firmware', productId ?? 'none'],
    queryFn: () => request<Firmware[]>(`/vendor/products/${productId}/firmware`),
    enabled: Boolean(productId),
  });
}

export function useAddFirmware(productId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: {
      version: string;
      size: number;
      sha256: string;
      ed25519_sig: string;
      object_key: string;
    }) =>
      request<Firmware>(`/vendor/products/${productId}/firmware`, {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['firmware', productId ?? 'none'] }),
  });
}

export function useCampaigns() {
  return useQuery({
    queryKey: qk.campaigns,
    queryFn: () => request<CampaignRow[]>('/vendor/campaigns'),
    // A rollout moves on its own; the table is stale within seconds otherwise.
    refetchInterval: 15_000,
  });
}

export function useCreateCampaign() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { product_id: string; firmware_id: string; rollout_pct: number }) =>
      request<Campaign>('/vendor/campaigns', {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.campaigns }),
  });
}

export function useStartCampaign() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      request<{ queued: number }>(`/vendor/campaigns/${id}/start`, { method: 'POST' }),
    onSuccess: () => qc.invalidateQueries({ queryKey: qk.campaigns }),
  });
}

/**
 * Roll a campaign back: a new, already-running campaign that offers the previous
 * image to exactly the devices running what this one shipped.
 */
export function useRollbackCampaign() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      request<Rollback>(`/vendor/campaigns/${id}/rollback`, { method: 'POST' }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['campaigns'] });
      void qc.invalidateQueries({ queryKey: ['audit'] });
    },
  });
}

export function useHaltCampaign() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      request<{ skipped: number }>(`/vendor/campaigns/${id}/halt`, { method: 'POST' }),
    // The same key the list uses. A hand-written key that does not match is an
    // invalidation that silently does nothing — the vendor halts a rollout, sees
    // no change, and clicks again.
    onSettled: () => qc.invalidateQueries({ queryKey: qk.campaigns }),
  });
}
