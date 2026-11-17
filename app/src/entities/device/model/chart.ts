import type { Bucket } from './history';
import { unitSuffix, valueWithUnit } from '../../../shared/ui/units';

/**
 * What a chart of history is allowed to claim.
 *
 * Two numbers describe a chart, and they must describe *the drawing*: the scale printed at its edges, and the
 * sentence printed underneath. They were computed in two places with two different inputs — the sentence
 * pushed the device's current reading into the range, the axis did not — so a stale sensor read
 * "16.6–21.5 °C over the last day" above a line that never went above 18.2. A customer cannot tell which of
 * the two numbers is lying, and the honest answer was "neither, they are about different things".
 *
 * So: the band is computed from buckets with samples, and the current reading joins it **only when it is
 * inside the window**, which is exactly when the last bucket is fresh.
 */

/** Only a bucket with samples is a reading. The API sends `v: 0, n: 0` for "nothing measured here". */
export const measured = (bucket: Bucket): boolean => bucket.n > 0 && Number.isFinite(bucket.v);

export type Band = {
  low: number;
  high: number;
  /** `16.6–18.2 °C`, or `steady at 16.6 °C` when the line is flat — which is a fact, stated badly by a range. */
  text: string;
  /** The same two numbers, formatted for the axis edges. */
  min: string;
  max: string;
};

export function chartBand(
  buckets: Bucket[],
  unit?: string | null,
  /** The live reading, when it is part of the window the chart draws. */
  current?: number | null,
): Band | null {
  const values = buckets.filter(measured).map((b) => b.v);
  if (typeof current === 'number' && Number.isFinite(current)) values.push(current);
  if (values.length < 2) return null;

  const low = Math.min(...values);
  const high = Math.max(...values);
  // A tenth of a degree is information; a tenth of a percent is noise. Whole numbers are printed whole, and
  // a temperature keeps its decimal because 16.6 and 16.8 are different rooms.
  const dp = Number.isInteger(low) && Number.isInteger(high) ? 0 : 1;
  const suffix = unitSuffix(unit) ?? '';
  const text =
    Math.abs(high - low) < 0.05
      ? `steady at ${valueWithUnit(low.toFixed(dp), unit)}`
      : `${low.toFixed(dp)}–${high.toFixed(dp)}${suffix}`;

  return {
    low,
    high,
    text,
    min: valueWithUnit(low.toFixed(dp), unit),
    max: valueWithUnit(high.toFixed(dp), unit),
  };
}
