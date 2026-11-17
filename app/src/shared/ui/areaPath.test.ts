import { buildSegments } from './areaPath';
import type { Bucket } from '../../entities/device/model/history';

/** Only `n` decides whether a bucket holds a reading: the API sends `v: 0` for a bucket with no samples. */
const b = (t: string, v: number, n = 1): Bucket => ({ t, v, n });

// A 0..100 x-axis and an inverted 0..10 y-axis is enough to read the path by eye.
const x = (i: number) => i * 10;
const y = (v: number) => 10 - v;
const HEIGHT = 100;

describe('buildSegments', () => {
  it('draws one run when nothing is missing', () => {
    const { line, area, runs, lastIndex } = buildSegments(
      [b('2026-01-01T00:00:00Z', 1), b('2026-01-01T01:00:00Z', 2), b('2026-01-01T02:00:00Z', 3)],
      x,
      y,
      HEIGHT,
    );
    expect(runs).toBe(1);
    expect(line.startsWith('M0.0,9.0')).toBe(true);
    expect(line).not.toContain('NaN');
    expect(lastIndex).toBe(2);
    // The fill closes under the last reading, not under the axis end.
    expect(area.endsWith('L20.0,100 Z')).toBe(true);
    expect(area.startsWith('M0.0,100 L0.0,9.0')).toBe(true);
  });

  it('breaks the line at a bucket with no samples instead of drawing it as zero', () => {
    const { line, runs, area } = buildSegments(
      [
        b('2026-01-01T00:00:00Z', 1),
        b('2026-01-01T01:00:00Z', 2),
        b('2026-01-01T02:00:00Z', 0, 0),
        b('2026-01-01T03:00:00Z', 4),
        b('2026-01-01T04:00:00Z', 5),
      ],
      x,
      y,
      HEIGHT,
    );
    expect(runs).toBe(2);
    // Two `M` commands: the second stretch starts after the gap rather than connecting across it.
    expect(line.match(/M/g)).toHaveLength(2);
    // The empty bucket drew `v: 0` — the y of 10 on this inverted axis — and must not be in the path.
    expect(line).not.toContain('L20.0,10.0');
    // Each stretch gets its own fill, so the gap is not painted as measured.
    expect(area.match(/Z/g)).toHaveLength(2);
  });

  it('reports the last bucket that actually held a reading', () => {
    const { lastIndex } = buildSegments(
      [b('2026-01-01T00:00:00Z', 1), b('2026-01-01T01:00:00Z', 2), b('2026-01-01T02:00:00Z', 0, 0)],
      x,
      y,
      HEIGHT,
    );
    // Not 2: the marker belongs where the line ends, which is 01:00.
    expect(lastIndex).toBe(1);
  });

  it('draws nothing at all when every bucket is empty', () => {
    const { line, area, runs, lastIndex } = buildSegments(
      [b('2026-01-01T00:00:00Z', 0, 0), b('2026-01-01T01:00:00Z', 0, 0)],
      x,
      y,
      HEIGHT,
    );
    expect(line).toBe('');
    expect(area).toBe('');
    expect(runs).toBe(0);
    expect(lastIndex).toBe(-1);
  });

  it('ignores a value that is not a finite number', () => {
    const { line } = buildSegments(
      [b('2026-01-01T00:00:00Z', 1), b('2026-01-01T01:00:00Z', Number.NaN)],
      x,
      y,
      HEIGHT,
    );
    expect(line).not.toContain('NaN');
  });
});
