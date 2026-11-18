import { describe, expect, it } from '@jest/globals';

import type { Device } from '../../entities/device';
import { buildRows } from './rows';

/**
 * The pairing rule, which is the one piece of the dashboard's layout that is data rather than style.
 *
 * A grid built by hand looks right until a room has an odd number of devices, or exactly one: then
 * the last tile sits alone in a half-width column and looks like a card that failed to load. These
 * cases are the ones that actually happen in a real home.
 */
function device(id: string, online = true): Device {
  return {
    id,
    serial: `SN-${id}`,
    name: `Device ${id}`,
    product_key: 'hv_acme_bulb1',
    category: 'light',
    online,
    points: [],
  };
}

describe('dashboard rows', () => {
  it('pairs devices two at a time and keeps the odd one on its own row', () => {
    const rows = buildRows([{ title: 'Living Room', key: 'r1', data: [device('a'), device('b'), device('c')] }]);

    expect(rows.map((row) => row.kind)).toEqual(['room', 'tiles', 'tiles']);
    expect(rows[0]).toMatchObject({ title: 'Living Room', total: 3, online: 3 });
    expect(rows[1]).toMatchObject({ wide: false });
    expect((rows[1] as { devices: Device[] }).devices).toHaveLength(2);
    expect((rows[2] as { devices: Device[] }).devices).toHaveLength(1);
  });

  it('gives a room of one the whole row', () => {
    const rows = buildRows([{ title: 'Bedroom', key: 'r2', data: [device('a')] }]);
    expect(rows[1]).toMatchObject({ kind: 'tiles', wide: true });
    expect((rows[1] as { devices: Device[] }).devices).toHaveLength(1);
  });

  it('counts who is online per room, not per row', () => {
    const rows = buildRows([
      { title: 'Kitchen', key: 'r3', data: [device('a'), device('b', false), device('c', false), device('d')] },
    ]);
    expect(rows[0]).toMatchObject({ online: 2, total: 4 });
  });

  it('produces stable keys, so a socket update repaints a row instead of remounting the grid', () => {
    const sections = [{ title: 'Hall', key: 'r4', data: [device('a'), device('b')] }];
    expect(buildRows(sections).map((row) => row.key)).toEqual(['room:r4', 'tiles:r4:0']);
  });

  it('keeps an empty section out of the list entirely', () => {
    // `groupByRoom` already drops empty rooms; this is the guard for anyone who stops dropping them.
    const rows = buildRows([{ title: 'Loft', key: 'r5', data: [] }]);
    expect(rows).toEqual([{ kind: 'room', key: 'room:r5', title: 'Loft', online: 0, total: 0 }]);
  });
});

describe('a room with more devices than a screen shows', () => {
  it('pages a long room at eight and offers the rest', () => {
    const data = Array.from({ length: 40 }, (_, i) => device(`d${i}`));
    const rows = buildRows([{ title: 'Living Room', key: 'big', data }]);

    // Four rows of two, then the control — not twenty rows of two.
    const tileRows = rows.filter((r) => r.kind === 'tiles');
    expect(tileRows).toHaveLength(4);
    const more = rows[rows.length - 1];
    expect(more).toMatchObject({ kind: 'more', roomKey: 'big', hidden: 32 });
  });

  it('shows the whole room when it has been expanded, and stops offering more', () => {
    const data = Array.from({ length: 40 }, (_, i) => device(`d${i}`));
    const rows = buildRows([{ title: 'Living Room', key: 'big', data }], 2, new Set(['big']));

    expect(rows.filter((r) => r.kind === 'tiles')).toHaveLength(20);
    expect(rows.some((r) => r.kind === 'more')).toBe(false);
  });

  it('does not nag a room that fits', () => {
    const data = Array.from({ length: 8 }, (_, i) => device(`d${i}`));
    const rows = buildRows([{ title: 'Hall', key: 'small', data }]);
    expect(rows.some((r) => r.kind === 'more')).toBe(false);
    expect(rows.filter((r) => r.kind === 'tiles')).toHaveLength(4);
  });
});
