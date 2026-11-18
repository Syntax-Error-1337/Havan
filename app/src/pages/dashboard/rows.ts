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
  | { kind: 'tiles'; key: string; devices: Device[]; wide: boolean }
  | { kind: 'more'; key: string; roomKey: string; title: string; hidden: number };

/** How many tiles a room shows before it offers the rest. Eight is four rows on a phone — enough to see the shape of
 *  a room, short enough that a room of forty does not become the whole home screen. */
export const ROOM_PAGE = 8;

export function buildRows(
  sections: DeviceSection[],
  columns = 2,
  expanded: ReadonlySet<string> = new Set(),
): DashboardRow[] {
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
    // A room with more devices than the page shows gets a control at the end of it rather than a longer screen.
    // The home in the demo has 1,200 devices across four rooms; without this, one room is the entire dashboard and
    // the second room is a memory.
    const capped =
      expanded.has(section.key) || section.data.length <= ROOM_PAGE
        ? section.data
        : section.data.slice(0, ROOM_PAGE);

    if (section.data.length === 1 && columns === 2) {
      rows.push({
        kind: 'tiles',
        key: `tiles:${section.key}:0`,
        devices: section.data,
        wide: true,
      });
      continue;
    }

    for (let i = 0; i < capped.length; i += columns) {
      rows.push({
        kind: 'tiles',
        key: `tiles:${section.key}:${i}`,
        devices: capped.slice(i, i + columns),
        wide: false,
      });
    }

    if (section.data.length > ROOM_PAGE && !expanded.has(section.key)) {
      rows.push({
        kind: 'more',
        key: `more:${section.key}`,
        roomKey: section.key,
        title: section.title,
        hidden: section.data.length - ROOM_PAGE,
      });
    }
  }

  return rows;
}
