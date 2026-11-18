import { formatLocation, parseLocation } from './types';

/**
 * A home's coordinates, which exist for one reason: `Condition::Solar` is evaluated against them, and without them
 * the engine answers `false` — so a rule saying "after sunset" would never run and the rule editor hides the
 * option instead of offering a control that can only fail.
 */

describe('formatLocation', () => {
  it('shows a pair at about eleven metres, not the float the API returns', () => {
    expect(formatLocation(51.507391234, -0.1278123)).toBe('51.5074, -0.1278');
  });

  it('says "Not set" rather than 0, 0 — which is a real place in the Atlantic', () => {
    expect(formatLocation(null, null)).toBe('Not set');
    expect(formatLocation(undefined, undefined)).toBe('Not set');
    expect(formatLocation(51.5, null)).toBe('Not set');
  });
});

describe('parseLocation', () => {
  it('reads a normal pair', () => {
    expect(parseLocation('51.5074', '-0.1278')).toEqual({ lat: 51.5074, lon: -0.1278 });
  });

  it('treats two empty fields as "clear it", not as an error', () => {
    // A customer who moves house should be able to say "not any more".
    expect(parseLocation('  ', '')).toBeNull();
  });

  it('refuses a pair with one half missing', () => {
    // The service refuses it too; saying so before the round trip is the difference between a form that explains
    // itself and one that fails after a spinner.
    expect(parseLocation('51.5', '')).toBe('invalid');
    expect(parseLocation('', '-0.12')).toBe('invalid');
  });

  it('refuses what is not a number, and what is off the planet', () => {
    expect(parseLocation('north', 'west')).toBe('invalid');
    expect(parseLocation('91', '0')).toBe('invalid');
    expect(parseLocation('-91', '0')).toBe('invalid');
    expect(parseLocation('0', '181')).toBe('invalid');
    expect(parseLocation('0', 'Infinity')).toBe('invalid');
  });

  it('accepts the edges, because the poles and the date line are places', () => {
    expect(parseLocation('90', '180')).toEqual({ lat: 90, lon: 180 });
    expect(parseLocation('-90', '-180')).toEqual({ lat: -90, lon: -180 });
  });
});
