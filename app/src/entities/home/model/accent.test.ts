import { homeAccent } from './accent';

/**
 * A home's colour is a fact about the home, not a random decoration.
 *
 * Two rules worth keeping: it must be **stable** (same id, same colour, on every launch and every device) and it must
 * be **spread** (three homes in a list should not all come out the same hue). The v7 uuid's leading digits are a
 * timestamp, so a hash that only looks at the first character fails the second rule for years.
 */
describe('a home has its own colour', () => {
  it('gives the same home the same colour every time', () => {
    const id = '01a1d936-4e1a-7335-964b-11b6c6c170fe';
    expect(homeAccent(id)).toBe(homeAccent(id));
  });

  it('spreads a list of homes over more than one hue', () => {
    // Ids that differ only in their tail: exactly the case a first-character hash gets wrong.
    const ids = [
      '01a1d936-4e1a-7335-964b-11b6c6c170fe',
      '01a1d936-4e1a-7335-964b-11b6c6c170ff',
      '01a1d936-4e1a-7335-964b-11b6c6c17001',
      '01a1d936-4e1a-7335-964b-11b6c6c17002',
    ];
    expect(new Set(ids.map(homeAccent)).size).toBeGreaterThan(1);
  });

  it('answers a colour for a home that has not loaded yet', () => {
    // The header renders before `/homes` does; a throw here is a blank dashboard.
    expect(homeAccent(null)).toMatch(/^#[0-9A-F]{6}$/i);
    expect(homeAccent(undefined)).toMatch(/^#[0-9A-F]{6}$/i);
  });
});
