import type { Bucket } from './history';
import { chartBand, measured } from './chart';

const b = (v: number, n = 1): Bucket => ({ t: '2026-01-01T00:00:00Z', v, n });

describe('chartBand', () => {
  it('describes only the buckets that were measured', () => {
    const band = chartBand([b(16.6), b(18.2), b(0, 0)], '°C');
    expect(band?.text).toBe('16.6–18.2 °C');
    expect(band?.min).toBe('16.6 °C');
    expect(band?.max).toBe('18.2 °C');
  });

  it('lets the current reading in only when the caller says it is inside the window', () => {
    const buckets = [b(16.6), b(18.2)];
    expect(chartBand(buckets, '°C')?.text).toBe('16.6–18.2 °C');
    expect(chartBand(buckets, '°C', 21.5)?.text).toBe('16.6–21.5 °C');
  });

  it('says "steady at" rather than a range of two equal numbers', () => {
    expect(chartBand([b(21.5), b(21.5)], '°C')?.text).toBe('steady at 21.5 °C');
  });

  it('writes a percentage against the number', () => {
    expect(chartBand([b(54), b(58)], '%')?.text).toBe('54–58%');
  });

  it('returns null when there is not enough to describe', () => {
    expect(chartBand([b(16.6)], '°C')).toBeNull();
    expect(chartBand([b(0, 0), b(0, 0)], '°C')).toBeNull();
  });
});

describe('measured', () => {
  it('is false for a bucket with no samples, even though its value is 0', () => {
    expect(measured(b(0, 0))).toBe(false);
    expect(measured(b(0))).toBe(true);
  });
});
