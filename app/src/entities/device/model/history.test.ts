import { gapHours, heights, type Bucket } from './history';

const bucket = (v: number, n = 1): Bucket => ({ t: '2026-01-01T00:00:00Z', v, n });

/**
 * The chart's arithmetic, tested without a renderer: a bar chart that quietly
 * rescales itself is a chart that lies.
 */
/**
 * The chart draws the live reading detached when the history has stopped, and the card explains the gap in
 * words. Both read this, so the marker and the caption cannot disagree about whether there is a gap at all.
 */
describe('the gap between the last reading and now', () => {
  const at = (iso: string): Bucket[] => [{ t: iso, v: 20, n: 5 }];
  const now = Date.parse('2026-11-17T22:00:00Z');

  it('is not a gap while the bucket is still filling', () => {
    // One bucket behind "now" is the bucket being written, not missing data.
    expect(gapHours(at('2026-11-17T21:00:00Z'), 3600, now)).toBeNull();
  });

  it('is a gap once two buckets have passed', () => {
    expect(gapHours(at('2026-11-17T11:00:00Z'), 1440, now)).toBe(11);
  });

  it('says nothing when there is no history to be late', () => {
    expect(gapHours([], 3600, now)).toBeNull();
  });

  it('never reports zero hours as a gap', () => {
    // A round to 0 would print "0 h without a reading", which is not a sentence.
    const hours = gapHours(at('2026-11-17T21:20:00Z'), 60, now);
    expect(hours === null || hours >= 1).toBe(true);
  });
});

describe('chart heights', () => {
  it('puts zero at the bottom for a boolean', () => {
    // A switch that was on half the hour must not draw as a full-height bar.
    expect(heights([bucket(1), bucket(0.5), bucket(0)], true)).toEqual([1, 0.5, 0]);
  });

  it('ignores a device that only ever reported one value', () => {
    // All readings equal means no span to draw — every bar is full, not NaN.
    expect(heights([bucket(21), bucket(21), bucket(21)], false)).toEqual([1, 1, 1]);
  });

  it('scales numbers from zero, not from the minimum', () => {
    // 18° and 21° must look close together; scaling from 18 would make them
    // look like a cliff.
    const bars = heights([bucket(18), bucket(21)], false);
    expect(bars[0]).toBeCloseTo(18 / 21);
    expect(bars[1]).toBe(1);
  });

  it('draws silence as a gap, not as zero', () => {
    // n === 0 means the device was not reporting. A zero-height bar in an
    // accent colour would claim it reported "off".
    const bars = heights([bucket(1), bucket(0, 0), bucket(1)], true);
    expect(bars).toEqual([1, null, 1]);
  });

  it('handles an empty history without inventing a bar', () => {
    expect(heights([], true)).toEqual([]);
  });

  it('handles negative readings', () => {
    const bars = heights([bucket(-10), bucket(10)], false);
    expect(bars[0]).toBe(0);
    expect(bars[1]).toBe(1);
  });
});
