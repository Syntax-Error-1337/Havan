import { describe, expect, it } from '@jest/globals';

import type { Device } from '../../entities/device';
import { categoriesOf, filterDevices, matchesQuery } from './search';

/**
 * Searching a home, which is a different problem from searching a catalogue: every device is already
 * on the machine, the words people type are the ones printed on their shelf, and the failure that
 * matters is being unable to find a device you are looking at.
 */
function device(over: Partial<Device> & { id: string }): Device {
  return {
    serial: `SN-${over.id}`,
    name: `Device ${over.id}`,
    product_key: 'hv_acme_bulb1',
    category: 'light',
    online: true,
    points: [],
    ...over,
  };
}

const lamp = device({ id: 'a', name: 'Floor Lamp', room_name: 'Living Room' });
const sensor = device({ id: 'b', name: 'Room Sensor', room_name: 'Bedroom', category: 'sensor' });
const accented = device({ id: 'c', name: 'Lämpchen', room_name: 'Küche' });

describe('device search', () => {
  it('finds by name, ignoring case', () => {
    expect(matchesQuery(lamp, 'floor')).toBe(true);
    expect(matchesQuery(lamp, 'FLOOR')).toBe(true);
    expect(matchesQuery(lamp, 'floor lamp')).toBe(true);
    expect(matchesQuery(lamp, 'bedroom')).toBe(false);
  });

  it('finds by serial, which is what is printed on the device and what support asks for', () => {
    // The serial was not in the haystack: a customer typing the number off the bottom of a bulb got nothing,
    // on a device that was on the screen behind the search box.
    expect(matchesQuery(device({ id: 'x', serial: 'HUGE-01199' }), 'HUGE-01199')).toBe(true);
    expect(matchesQuery(device({ id: 'y', serial: 'HUGE-01199' }), '01199')).toBe(true);
  });

  it('finds by the room a device is in, which is how people describe it', () => {
    expect(matchesQuery(sensor, 'bedroom')).toBe(true);
    expect(matchesQuery(lamp, 'living')).toBe(true);
  });

  it('finds by category, and not by the vendor product key unless asked', () => {
    expect(matchesQuery(sensor, 'sensor')).toBe(true);
    expect(matchesQuery(lamp, 'light')).toBe(true);
    // The product key is searchable, but nobody types it — it is there so a support agent can paste
    // one from a ticket.
    expect(matchesQuery(lamp, 'hv_acme_bulb1')).toBe(true);
  });

  it('folds accents, because nobody types them into a search box', () => {
    expect(matchesQuery(accented, 'lampchen')).toBe(true);
    expect(matchesQuery(accented, 'kuche')).toBe(true);
  });

  it('narrows across words instead of widening', () => {
    // "lamp living" is a device in the living room called lamp; "lamp bedroom" is nobody.
    expect(matchesQuery(lamp, 'lamp living')).toBe(true);
    expect(matchesQuery(lamp, 'lamp bedroom')).toBe(false);
  });

  it('an empty query is every device, not none', () => {
    expect(matchesQuery(lamp, '')).toBe(true);
    expect(matchesQuery(lamp, '   ')).toBe(true);
  });

  it('filters by category and query together', () => {
    const all = [lamp, sensor, accented];
    expect(filterDevices(all, '', null)).toHaveLength(3);
    expect(filterDevices(all, '', 'sensor').map((d) => d.id)).toEqual(['b']);
    expect(filterDevices(all, 'lamp', null).map((d) => d.id)).toEqual(['a', 'c']);
    expect(filterDevices(all, 'lamp', 'sensor')).toHaveLength(0);
  });

  it('lists the categories present, most familiar first', () => {
    const all = [lamp, sensor, device({ id: 'd', category: 'vacuum' }), device({ id: 'e', category: 'plug' })];
    expect(categoriesOf(all)).toEqual(['light', 'sensor', 'plug', 'vacuum']);
  });
});
