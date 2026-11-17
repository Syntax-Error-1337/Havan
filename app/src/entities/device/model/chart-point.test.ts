import { chartPoint, RANGE_WORDS } from './history';

/**
 * Which series the device screen charts.
 *
 * The rule this replaces was "the first numeric point", which on a sensor is the battery — so the one
 * screen a customer opens to see the temperature drew a battery percentage instead, and the temperature
 * had no chart at all.
 */
describe('chartPoint', () => {
  const point = (capability: string, value: unknown = 1) => ({ capability, value });

  it('prefers the reading the screen exists for', () => {
    const points = [point('battery', 87), point('temperature', 21.5), point('humidity', 58)];
    expect(chartPoint(points)?.capability).toBe('temperature');
  });

  it('falls back through the hierarchy when the first choice is absent', () => {
    expect(chartPoint([point('battery', 87), point('humidity', 58)])?.capability).toBe('humidity');
    expect(chartPoint([point('battery', 87), point('power', 12)])?.capability).toBe('power');
  });

  it('keeps a battery as the last resort, rather than falling back to order', () => {
    expect(chartPoint([point('battery', 87)])?.capability).toBe('battery');
  });

  it('still charts something for a product with no capabilities we recognise', () => {
    const custom = [{ code: 'co2.ppm', value: 600 }, { code: 'label', value: 'x' }];
    expect(chartPoint(custom)?.code).toBe('co2.ppm');
    expect(chartPoint([])).toBeUndefined();
  });
});

describe('RANGE_WORDS', () => {
  it('names each window as a period a sentence can use', () => {
    expect(RANGE_WORDS['1d']).toBe('day');
    expect(RANGE_WORDS['7d']).toBe('week');
    expect(RANGE_WORDS['30d']).toBe('30 days');
  });
});
