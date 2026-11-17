import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { adminRequest } from '../../shared/api/admin';

/** The operator's queries live under their own key space, well away from the vendor cache. */
export const adminKeys = {
  overview: ['admin', 'overview'] as const,
  vendors: ['admin', 'vendors'] as const,
  queue: ['admin', 'certification'] as const,
  health: ['admin', 'health'] as const,
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

export function useVendor(id: string) {
  return useQuery({
    queryKey: [...adminKeys.vendors, id] as const,
    queryFn: () => adminRequest<VendorRow>(`/ops/vendors/${id}`),
    retry: false,
  });
}

export type VendorLicences = {
  summary: {
    granted: number;
    used: number;
    remaining: number;
    activated_this_period: number;
    granted_value_cents: number;
    used_value_cents: number;
    unit_price_cents: number;
    currency: string;
  };
  activations: {
    serial: string;
    product_key: string;
    product_name: string;
    activated_at: string;
    unit_price_cents: number;
  }[];
};

/** The operator sees a vendor's ledger, which is the same read the vendor portal does. */
export function useVendorLicences(id: string) {
  return useQuery({
    queryKey: [...adminKeys.vendors, id, 'licences'] as const,
    queryFn: () => adminRequest<VendorLicences>(`/ops/licences?vendor_id=${id}&limit=20`),
    retry: false,
  });
}

export type VendorInvoice = {
  period: string;
  currency: string;
  quantity: number;
  amount_cents: number;
  status: 'draft' | 'issued';
  issued_at: string | null;
};

export function useVendorInvoice(id: string, period: string) {
  return useQuery({
    queryKey: [...adminKeys.vendors, id, 'invoice', period] as const,
    queryFn: () =>
      adminRequest<VendorInvoice>(
        `/ops/invoices?vendor_id=${id}&period=${encodeURIComponent(period)}`,
      ),
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

export type QuietDevice = {
  serial: string | null;
  name: string;
  vendor: string;
  last_seen_at: string | null;
  silent_minutes: number;
};

export type StuckCommand = {
  device: string | null;
  vendor: string;
  op: string;
  attempts: number;
  requested_at: string;
};

export type PlatformHealth = {
  devices: number;
  online: number;
  seen_recently: number;
  points_last_hour: number;
  commands_last_hour: number;
  undelivered: number;
  undelivered_notifications: number;
  webhook_backlog: number;
  quietest: QuietDevice[];
  stuck_commands: StuckCommand[];
};

export function useHealth() {
  return useQuery({
    queryKey: adminKeys.health,
    queryFn: () => adminRequest<PlatformHealth>('/ops/health'),
    retry: false,
    // A health page that shows numbers from a minute ago is a health page that lies at 3am.
    refetchInterval: 15_000,
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

export type PurgeReport = { purged: string[] };

/**
 * Erased accounts past their retention window.
 *
 * `POST /ops/privacy/purge` had a route and no screen, so the only way to run a data-deletion purge
 * was curl. The number comes first: "purge" with nothing in front of it is a command nobody can
 * sanity-check, and a number that is not zero is itself the finding — the hourly worker should keep
 * it at zero.
 */
export function usePrivacyDue() {
  return useQuery({
    queryKey: ['admin', 'privacy', 'due'] as const,
    queryFn: () => adminRequest<{ due: number }>('/ops/privacy/due'),
  });
}

export function usePurge() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => adminRequest<PurgeReport>('/ops/privacy/purge', { method: 'POST' }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['admin', 'privacy'] }),
  });
}

export type AuditEntryRow = {
  id: string;
  ts: string;
  actor_kind: string;
  actor_id: string | null;
  action: string;
  target_kind: string;
  target_id: string | null;
  request_id: string | null;
  ip: string | null;
  detail: Record<string, unknown>;
};

export type AuditPage = { entries: AuditEntryRow[]; next_cursor: string | null };

/**
 * Who did what, across every vendor.
 *
 * The operator could suspend a vendor and certify a product with no record a person could read: the
 * audit log was per vendor, and the console has no vendor to scope to. This is the same page widened,
 * so an operator's answer and a vendor's cannot disagree.
 */
export function useAuditPage(before?: string) {
  return useQuery({
    queryKey: ['admin', 'audit', before ?? 'newest'] as const,
    queryFn: () =>
      adminRequest<AuditPage>(`/ops/audit?limit=100${before ? `&before=${before}` : ''}`),
  });
}
