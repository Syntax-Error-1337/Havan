import { signalAccent, type SignalKind } from '../../../shared/config/tokens';

/**
 * A home's own hue, from its id.
 *
 * Every home drew the same blue initial, so a list of three homes was three identical circles with different letters —
 * the one screen whose whole job is telling homes apart. The four signal hues are already in the palette (automation,
 * ota, security, alert), which means this is a *choice among the operator's colours* rather than a colour invented at
 * runtime: an operator who changes the theme changes these too, and the token gate stays honest.
 *
 * Deterministic on the id, so a home keeps its colour across launches and devices — a colour that changes on every
 * render is decoration, not identity.
 */
const HUES: SignalKind[] = ['automation', 'ota', 'security', 'alert'];

export function homeAccent(homeId: string | null | undefined): string {
  if (!homeId) return signalAccent.automation;
  // FNV-1a over the id: tiny, stable, and not `charCodeAt(0)` — every v7 uuid starts with the same timestamp digits,
  // so the first character would put every home in the same bucket for years.
  let hash = 0x811c9dc5;
  for (let i = 0; i < homeId.length; i += 1) {
    hash ^= homeId.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return signalAccent[HUES[hash % HUES.length]!];
}
