import type { Bucket } from '../../entities/device/model/history';

/**
 * The path for a line drawn through buckets of readings, some of which are missing.
 *
 * This lived inside `AreaChart` as a `forEach` that concatenated every bucket, and it drew a
 * `L NaN,NaN` for every gap: the comment above it claimed "gaps are honoured — a bucket with no reading
 * breaks the line instead of being drawn as zero", and the code did neither. On a device that stopped
 * reporting nine hours ago, the last nine points were NaN, the renderer dropped them, and the chart ended
 * with a floating dot at the last non-null value placed at the x of a bucket that had no value.
 *
 * A pure function, so the thing that was wrong can be asserted in a test rather than inspected in a
 * screenshot.
 */
export type Segments = {
  /** One `M…L…` run per unbroken stretch of readings. */
  line: string;
  /** The fill under each run, closing to the baseline under its own last reading. */
  area: string;
  /** The index of the last bucket that actually held a reading, or -1. */
  lastIndex: number;
  /** How many separate stretches the line is drawn in — more than one means there is a gap in it. */
  runs: number;
};

const finite = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);

/**
 * A bucket with `n === 0` is a bucket with **no samples** — the API sends `v: 0` for it, which is why the old
 * code drew a sensor that had gone quiet as a line falling to zero and staying there. `n` is the count; it is
 * the only field that means "nothing was measured here".
 */
const missing = (bucket: Bucket): boolean => bucket.n === 0 || !finite(bucket.v);

export function buildSegments(buckets: Bucket[], x: (i: number) => number, y: (v: number) => number, height: number): Segments {
  let line = '';
  let area = '';
  let run: { px: number; py: number }[] = [];
  let runs = 0;
  let lastIndex = -1;

  const flush = () => {
    if (run.length === 0) return;
    runs += 1;
    const first = run[0]!;
    const last = run[run.length - 1]!;
    line += (line ? ' ' : '') + `M${first.px.toFixed(1)},${first.py.toFixed(1)}`;
    for (const p of run.slice(1)) line += ` L${p.px.toFixed(1)},${p.py.toFixed(1)}`;
    // The fill closes under the run's last reading, not under the axis end: filling to the edge would paint
    // the gap as if it had been measured.
    area += `M${first.px.toFixed(1)},${height} L${first.px.toFixed(1)},${first.py.toFixed(1)}`;
    for (const p of run.slice(1)) area += ` L${p.px.toFixed(1)},${p.py.toFixed(1)}`;
    area += ` L${last.px.toFixed(1)},${height} Z `;
    run = [];
  };

  buckets.forEach((bucket, i) => {
    if (missing(bucket)) {
      // A missing reading ends the stretch. The next reading starts a new one, with its own `M`.
      flush();
      return;
    }
    lastIndex = i;
    run.push({ px: x(i), py: y(bucket.v) });
  });
  flush();

  return { line: line.trim(), area: area.trim(), lastIndex, runs };
}
