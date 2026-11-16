import { useQuery } from '@tanstack/react-query';

import { request } from '../../shared/api/client';
import { qk } from '../../shared/api/keys';

/**
 * A month of activations: a draft, or the document once it was issued.
 *
 * `status` is the field the screen turns on. A draft changes when a device activates late, and
 * saying so is honest; an issued document never changes, and a vendor paying against it needs to
 * know which one they are looking at.
 */
export type InvoiceLine = {
  serial: string;
  product_key: string;
  unit_price_cents: number;
  activated_at: string;
};

export type Invoice = {
  period: string;
  currency: string;
  quantity: number;
  amount_cents: number;
  status: 'draft' | 'issued';
  issued_at: string | null;
  lines: InvoiceLine[];
};

/** The current month, in UTC — the same month the platform would invoice you for. */
export function currentPeriod(): string {
  const now = new Date();
  return `${now.getUTCFullYear()}-${String(now.getUTCMonth() + 1).padStart(2, '0')}`;
}

export function useInvoice(period: string) {
  return useQuery({
    queryKey: [...qk.invoices, period] as const,
    queryFn: () => request<Invoice>(`/vendor/invoices?period=${encodeURIComponent(period)}`),
  });
}
