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

/**
 * How long the readings have been missing, in whole hours — `null` when the last bucket is still fresh.
 *
 * The chart draws the live reading as a marker detached from the line, and the footer says why. Both need the
 * same answer to "is there a gap?", so it is computed once, here, and both read it: a marker explained by one
 * number and a caption computed from another is how a card ends up contradicting itself.
 *
 * "Fresh" means within two buckets: a bucket that is simply still being filled is not a gap.
 */
export function gapHours(
  buckets: Bucket[],
  bucketSecs: number,
  now: number = Date.now(),
): number | null {
  const last = buckets[buckets.length - 1];
  if (!last) return null;
  const at = new Date(last.t).getTime();
  if (Number.isNaN(at)) return null;
  const silentMs = now - at;
  if (silentMs <= (bucketSecs || 3600) * 1000 * 2) return null;
  return Math.max(1, Math.round(silentMs / 3_600_000));
}

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
 * Which point the device screen charts.
 *
 * It used to be "the first numeric one", which on a sensor is `battery.level` — so the screen drew a
 * battery percentage falling by one over a week and called it the device's history, while the
 * temperature it exists to measure had no chart at all. Preference, not order: the reading a person
 * opened the screen for, then the ones they can act on.
 *
 * Battery stays last on purpose: it is the interesting series exactly once, when it is nearly empty.
 */
export const CHART_PREFERENCE = [
  'temperature',
  'humidity',
  'energy',
  'power',
  'brightness',
  'battery',
] as const;

/** The window in words, for the sentence that explains an empty chart. */
export const RANGE_WORDS: Record<Range, string> = {
  '1h': 'hour',
  '1d': 'day',
  '7d': 'week',
  '30d': '30 days',
};

export function chartPoint<T extends { capability?: string; value?: unknown }>(
  points: T[],
): T | undefined {
  for (const capability of CHART_PREFERENCE) {
    const match = points.find((p) => p.capability === capability);
    if (match) return match;
  }
  return points.find((p) => typeof p.value === 'number') ?? points[0];
}
