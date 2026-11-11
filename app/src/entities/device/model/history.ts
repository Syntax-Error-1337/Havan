import { useQuery } from '@tanstack/react-query';

import { request } from '../../../shared/api/client';
import { qk } from '../../../shared/api/keys';

export type Bucket = { t: string; v: number; n: number };

export type History = {
  device_id: string;
  code: string;
  from: string;
  to: string;
  bucket_secs: number;
  buckets: Bucket[];
};

export const RANGES = ['1h', '1d', '7d', '30d'] as const;
export type Range = (typeof RANGES)[number];

export function useHistory(deviceId: string | undefined, code: string | undefined, range: Range) {
  return useQuery({
    queryKey: qk.history(deviceId ?? 'none', code ?? 'none', range),
    queryFn: () =>
      request<History>(
        `/devices/${deviceId}/history?code=${encodeURIComponent(code ?? '')}&range=${range}`,
      ),
    enabled: Boolean(deviceId && code),
    // History is written by the device, not by the screen: refetching on focus
    // is all the freshness a chart needs.
    staleTime: 30_000,
  });
}

/**
 * Turn buckets into 0–1 heights for drawing.
 *
 * Two rules that keep the chart honest:
 *
 * - the scale includes **zero** for booleans, so "off" is a flat line at the
 *   bottom rather than a bar that looks half-on;
 * - a bucket with no samples is a gap, not a zero. Drawing silence as zero is
 *   how a chart lies about a device that was offline.
 */
export function heights(buckets: Bucket[], isBoolean: boolean): (number | null)[] {
  if (buckets.length === 0) return [];

  const values = buckets.map((b) => b.v);
  const max = isBoolean ? 1 : Math.max(...values);
  const min = isBoolean ? 0 : Math.min(0, ...values);
  const span = max - min || 1;

  return buckets.map((bucket) =>
    bucket.n === 0 ? null : Math.min(1, Math.max(0, (bucket.v - min) / span)),
  );
}
