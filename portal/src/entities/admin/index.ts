import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { adminRequest } from '../../shared/api/admin';

/** The operator's queries live under their own key space, well away from the vendor cache. */
export const adminKeys = {
  overview: ['admin', 'overview'] as const,
  vendors: ['admin', 'vendors'] as const,
  queue: ['admin', 'certification'] as const,
};

export type VendorRow = {
  id: string;
  slug: string;
  name: string;
  status: 'active' | 'suspended' | 'deleted';
  quota_devices: number | null;
  totp_required: boolean;
  products: number;
  devices: number;
  granted: number;
  used: number;
  created_at: string;
};

export type Overview = {
  vendors: number;
  suspended: number;
  awaiting_certification: number;
  invoices_this_month: number;
};

export type QueueItem = {
  product_id: string;
  vendor_id: string;
  vendor_name: string;
  product_key: string;
  name: string;
  category: string;
  submitted_at: string | null;
  note: string | null;
};

export function useOverview() {
  return useQuery({
    queryKey: adminKeys.overview,
    queryFn: () => adminRequest<Overview>('/ops/overview'),
    // A refused token is an answer, not a blip: retrying it three times makes the console feel broken.
    retry: false,
  });
}

export function useVendors() {
  return useQuery({
    queryKey: adminKeys.vendors,
    queryFn: () => adminRequest<VendorRow[]>('/ops/vendors'),
    retry: false,
  });
}

export function useCreateVendor() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { slug: string; name: string; quota_devices?: number }) =>
      adminRequest<VendorRow>('/ops/vendors', {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: adminKeys.vendors });
      qc.invalidateQueries({ queryKey: adminKeys.overview });
    },
  });
}

/** Suspend, resume, rename or set a ceiling. `quota_devices: null` clears it. */
export function usePatchVendor() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: {
      id: string;
      status?: 'active' | 'suspended';
      quota_devices?: number | null;
      name?: string;
    }) => {
      const { id, ...patch } = input;
      return adminRequest<VendorRow>(`/ops/vendors/${id}`, {
        method: 'PATCH',
        body: JSON.stringify(patch),
      });
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: adminKeys.vendors });
      qc.invalidateQueries({ queryKey: adminKeys.overview });
    },
  });
}

export function useCertificationQueue() {
  return useQuery({
    queryKey: adminKeys.queue,
    queryFn: () => adminRequest<QueueItem[]>('/ops/certification/queue'),
    retry: false,
  });
}

export function useDecideCertification() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: { productId: string; decision: 'certify' | 'revoke'; note?: string }) =>
      adminRequest<unknown>(`/ops/products/${input.productId}/certification/${input.decision}`, {
        method: 'POST',
        body: JSON.stringify({ note: input.note }),
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: adminKeys.queue });
      qc.invalidateQueries({ queryKey: adminKeys.overview });
    },
  });
}

export function useGrantLicences() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: {
      vendor_id: string;
      quantity: number;
      unit_price_cents: number;
      currency?: string;
    }) =>
      adminRequest<{ id: string; summary: unknown }>('/ops/licences', {
        method: 'POST',
        body: JSON.stringify(input),
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: adminKeys.vendors }),
  });
}

export function useIssueInvoice() {
  return useMutation({
    mutationFn: (input: { vendor_id: string; period: string }) =>
      adminRequest<{ period: string; status: string; quantity: number; amount_cents: number }>(
        '/ops/invoices',
        { method: 'POST', body: JSON.stringify(input) },
      ),
  });
}

/** The CSV export is a download, not a query: a browser handles the file better than JSON would. */
export function invoiceCsvUrl(vendorId: string, period: string): string {
  const base = import.meta.env.VITE_API_URL ?? '/v1';
  return `${base}/ops/invoices.csv?vendor_id=${encodeURIComponent(vendorId)}&period=${encodeURIComponent(period)}`;
}
