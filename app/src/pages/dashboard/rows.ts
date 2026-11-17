import type { Device } from '../../entities/device';
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

export function buildRows(sections: DeviceSection[]): DashboardRow[] {
  const rows: DashboardRow[] = [];

  for (const section of sections) {
    rows.push({
      kind: 'room',
      key: `room:${section.key}`,
      title: section.title,
      online: section.data.filter((device) => device.online).length,
      total: section.data.length,
    });

    if (section.data.length === 1) {
      rows.push({
        kind: 'tiles',
        key: `tiles:${section.key}:0`,
        devices: section.data,
        wide: true,
      });
      continue;
    }

    for (let i = 0; i < section.data.length; i += 2) {
      rows.push({
        kind: 'tiles',
        key: `tiles:${section.key}:${i}`,
        devices: section.data.slice(i, i + 2),
        wide: false,
      });
    }
  }

  return rows;
}
