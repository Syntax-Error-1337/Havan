import { useQuery } from '@tanstack/react-query';

import { request } from '../../shared/api/client';
import { qk } from '../../shared/api/keys';

/**
 * What the vendor bought and what they have spent.
 *
 * `remaining` is deliberately not clamped at zero in the type: a negative balance means the
 * operator revoked a grant after devices were live, and a screen that quietly shows 0 would hide
 * the one number somebody needs to see.
 */
export type LicenceSummary = {
  granted: number;
  used: number;
  remaining: number;
  activated_this_period: number;
  granted_value_cents: number;
  used_value_cents: number;
  unit_price_cents: number;
  currency: string;
};

export type Activation = {
  serial: string;
  product_key: string;
  product_name: string;
  activated_at: string;
  unit_price_cents: number;
};

export type LicencePage = {
  summary: LicenceSummary;
  activations: Activation[];
};

export function useLicences() {
  return useQuery({
    queryKey: qk.licences,
    queryFn: () => request<LicencePage>('/vendor/licences'),
  });
}

/** Money in cents, rendered the way a vendor reads it: 24900 → 249.00. */
export function formatMoney(cents: number, currency: string): string {
  const amount = (cents / 100).toFixed(2);
  return `${amount} ${currency}`;
}
