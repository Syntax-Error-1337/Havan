import { gapHours, type Bucket } from './history';

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

