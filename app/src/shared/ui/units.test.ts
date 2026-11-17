import { unitSuffix, valueWithUnit } from './units';

describe('valueWithUnit', () => {
  it('writes a percent sign against the number, like the phone it is drawn on', () => {
    expect(valueWithUnit(58, '%')).toBe('58%');
    expect(valueWithUnit('58', 'pct')).toBe('58%');
    expect(valueWithUnit(58, 'percent')).toBe('58%');
  });

  it('keeps the space a temperature needs, because °C is a symbol and not a suffix', () => {
    expect(valueWithUnit('21.5', '°C')).toBe('21.5 °C');
    expect(valueWithUnit(4, 'kWh')).toBe('4 kWh');
  });

  it('does not invent a space before a degree sign on its own', () => {
    expect(valueWithUnit(21.5, '°')).toBe('21.5°');
  });

  it('returns the bare value when there is no unit, or only whitespace', () => {
    expect(valueWithUnit(7, null)).toBe('7');
    expect(valueWithUnit(7, '   ')).toBe('7');
  });

  it('suffixes the same way, for the second text node beside the number', () => {
    expect(unitSuffix('%')).toBe('%');
    expect(unitSuffix('°C')).toBe(' °C');
    expect(unitSuffix(null)).toBeNull();
  });
});
