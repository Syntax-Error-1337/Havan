import type { Device } from '../../entities/device';

/**
 * Finding one device in a house that has a lot of them.
 *
 * The grid is fine at ten devices and hostile at two hundred: the thing you want is always below the
 * fold, and scroll position is not a search. This is deliberately a plain in-memory filter rather
 * than server-side search — the dashboard already carries the whole home, so asking the network to
 * re-answer a question it has already answered would be slower and would flicker.
 *
 * What it matches, in order of how people actually type:
 * - the device's name ("floor lamp")
 * - **its serial**, which is what is printed on the thing in the customer's hand and the first thing support
 *   asks for — it was missing, so a serial typed into the box matched nothing even when the device was loaded
 * - the room it is in ("bedroom")
 * - its category ("light", "sensor") — a word people use, unlike `hv_acme_bulb1`
 *
 * Accents and case are folded: someone typing "lamp" should find "Lämp", and nobody types capitals
 * into a search box.
 */
export function matchesQuery(device: Device, query: string): boolean {
  const needle = fold(query);
  if (!needle) return true;

  // Every word has to match something, so "lamp bedroom" narrows instead of widening.
  return needle
    .split(/\s+/)
    .filter(Boolean)
    .every((word) =>
      [device.name, device.serial ?? '', device.room_name ?? '', device.category, device.product_key]
        .map(fold)
        .some((haystack) => haystack.includes(word)),
    );
}

/** Lower-case and strip diacritics, so "Lämp" and "lamp" are the same word to a search box. */
export function fold(value: string): string {
  return value
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .trim();
}

export function filterDevices(
  devices: Device[],
  query: string,
  category: string | null,
): Device[] {
  return devices.filter(
    (device) =>
      (category === null || device.category === category) && matchesQuery(device, query),
  );
}

/** The categories that exist in this home, in the order a person scans them. */
export function categoriesOf(devices: Device[]): string[] {
  const preferred = ['light', 'climate', 'sensor', 'plug', 'cover', 'lock', 'camera'];
  const present = new Set(devices.map((device) => device.category));
  return [
    ...preferred.filter((category) => present.has(category)),
    ...[...present].filter((category) => !preferred.includes(category)).sort(),
  ];
}
