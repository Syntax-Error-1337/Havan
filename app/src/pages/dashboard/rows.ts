import { freshness, type Device } from '../../entities/device';
import type { DeviceSection } from './sections';

/**
 * Rooms and devices flattened into rows a windowed list can render.
 *
 * This is the only part of the dashboard that is pure data, which is exactly why it is out here: the
 * pairing rule (a room of one gives its tile the whole row, everything else pairs up) is easy to get
 * subtly wrong — an odd device left alone in a 47 % column — and easy to prove right.
 */
export type DashboardRow =
  | { kind: 'room'; key: string; title: string; online: number; total: number }
  | { kind: 'tiles'; key: string; devices: Device[]; wide: boolean };

export function buildRows(sections: DeviceSection[], columns = 2): DashboardRow[] {
  const rows: DashboardRow[] = [];

  for (const section of sections) {
    rows.push({
      kind: 'room',
      key: `room:${section.key}`,
      title: section.title,
      // The same rule the tiles and the header use. A room header claiming "1/2 online" above a tile
      // whose dot is amber is two answers to one question, and the customer believes neither.
      online: section.data.filter((device) => freshness(device) === 'live').length,
      total: section.data.length,
    });

    // A room holding **one** device gives it the whole row: a lone half-width card reads as a missing tile. A room
    // holding two on a three-column grid keeps the column width and leaves the third empty — stretching them to fill
    // the row was the first attempt, and it made two devices in the same house twice the size of three.
    // On a three-column tablet a lone device takes one column like everything else: a full-width card next to a
    // third-width one is two tile sizes in one house. Full row is a phone answer.
    if (section.data.length === 1 && columns === 2) {
      rows.push({
        kind: 'tiles',
        key: `tiles:${section.key}:0`,
        devices: section.data,
        wide: true,
      });
      continue;
    }

    for (let i = 0; i < section.data.length; i += columns) {
      rows.push({
        kind: 'tiles',
        key: `tiles:${section.key}:${i}`,
        devices: section.data.slice(i, i + columns),
        wide: false,
      });
    }
  }

  return rows;
}
