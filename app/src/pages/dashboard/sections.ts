import type { Device } from '../../entities/device';
import type { Room } from '../../entities/home';

export type DeviceSection = {
  title: string;
  key: string;
  data: Device[];
};

/**
 * The dashboard's sections: rooms that hold something, plus an "Unassigned"
 * bucket for devices nobody has placed yet.
 *
 * Two rules that matter more than they look:
 *
 * - An empty room is not shown. A row of nothing is a promise the user made and
 *   did not keep; it reads as a bug.
 * - A device pointing at a room that no longer exists still appears, under "No
 *   longer roomed". Deleting a room must never make a device disappear from the
 *   only screen that can reach it.
 */
export function groupByRoom(
  devices: Device[],
  rooms: Room[] | undefined,
): DeviceSection[] {
  const known = new Set((rooms ?? []).map((room) => room.id));
  const byRoom = new Map<string | null, Device[]>();

  for (const device of devices) {
    const id = device.room_id ?? null;
    const key = id !== null && !known.has(id) ? '__orphaned' : id;
    byRoom.set(key, [...(byRoom.get(key) ?? []), device]);
  }

  const sections: DeviceSection[] = (rooms ?? [])
    .map((room) => ({
      title: room.name,
      key: room.id,
      data: byRoom.get(room.id) ?? [],
    }))
    .filter((section) => section.data.length > 0);

  const loose = byRoom.get(null) ?? [];
  const orphaned = byRoom.get('__orphaned') ?? [];

  return [
    ...(loose.length > 0 ? [{ title: 'Unassigned', key: 'unassigned', data: loose }] : []),
    ...(orphaned.length > 0
      ? [{ title: 'No longer roomed', key: 'orphaned', data: orphaned }]
      : []),
    ...sections,
  ];
}
