/**
 * A value and its unit, the way the platform it is drawn on writes them.
 *
 * `58 %` is what you get when every unit is a string concatenated with a space, and it is wrong twice: Apple
 * writes `58%`, and the space before a percent sign is an English-typography rule from forty years of running
 * text, not a rule for a reading. Temperature is the opposite — `21.5 °C` takes the space, because `°C` is a
 * separate symbol, not a suffix.
 *
 * One function, so the dashboard, the tile and the detail screen cannot disagree about the same number.
 */
const NO_SPACE_BEFORE = new Set(['%', '°', "'", '"']);

/** True when the unit is a bare per-cent sign written by a vendor as `%`, `percent` or `pct`. */
function isPercent(unit: string): boolean {
  const trimmed = unit.trim();
  return trimmed === '%' || /^per\s?cent$/i.test(trimmed) || /^pct$/i.test(trimmed);
}

export function valueWithUnit(value: string | number, unit?: string | null): string {
  const text = String(value);
  if (!unit || !unit.trim()) return text;
  const trimmed = unit.trim();
  if (isPercent(trimmed)) return `${text}%`;
  if (NO_SPACE_BEFORE.has(trimmed)) return `${text}${trimmed}`;
  return `${text} ${trimmed}`;
}

/**
 * A reading, rounded to the precision a person reads at.
 *
 * `18.27 °C` is what a sensor sent, not what a room is: the extra decimal is false precision on a device whose
 * own accuracy is ±0.5, and it makes the number harder to scan. Whole numbers stay whole.
 */
export function reading(value: number): string {
  if (!Number.isFinite(value)) return '—';
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

/** The unit on its own, for the places that draw it in a second, quieter text node beside the number. */
export function unitSuffix(unit?: string | null): string | null {
  if (!unit || !unit.trim()) return null;
  const trimmed = unit.trim();
  if (isPercent(trimmed)) return '%';
  return NO_SPACE_BEFORE.has(trimmed) ? trimmed : ` ${trimmed}`;
}
