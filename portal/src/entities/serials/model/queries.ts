import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';

export type SerialSummary = { total: number; claimed: number; open: number };

export type UploadResult = { added: number; already_listed: number; summary: SerialSummary };

export const serialKeys = {
  summary: (productId: string) => ['vendor', 'product', productId, 'serials'] as const,
};

export function useSerials(productId: string) {
  return useQuery({
    queryKey: serialKeys.summary(productId),
    queryFn: () => request<SerialSummary>(`/vendor/products/${productId}/serials`),
    enabled: Boolean(productId),
    staleTime: 30_000,
  });
}

export function useUploadSerials(productId: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (serials: string[]) =>
      request<UploadResult>(`/vendor/products/${productId}/serials`, {
        method: 'POST',
        body: JSON.stringify({ serials }),
      }),
    onSuccess: (result) => qc.setQueryData(serialKeys.summary(productId), result.summary),
  });
}

/**
 * Turn a pasted list into serials.
 *
 * One per line is what a CSV export looks like; commas and semicolons are what
 * people paste when they have been in a spreadsheet. The server normalises again
 * — this is a convenience, not the gate.
 */
export function parseSerials(text: string): string[] {
  return text
    .split(/[\n,;\t]+/)
    .map((s) => s.trim().replace(/^"|"$/g, ''))
    .filter((s) => s.length > 0 && s.length <= 128);
}
