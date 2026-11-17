import { summarise } from './summary';
import type { Device } from './types';

const minutesAgo = (m: number): string => new Date(Date.now() - m * 60_000).toISOString();

const device = (over: Partial<Device> & Pick<Device, 'id' | 'category'>): Device => ({
  serial: over.id,
  name: over.id,
  product_key: 'hv_test',
  online: true,
  last_seen_at: minutesAgo(1),
  points: [],
  ...over,
});

const power = (on: boolean) => [
  { id: 'p', code: 'switch.power', capability: 'switch', slot: 'power', kind: 'bool' as const, writable: true, value: on },
];

describe('what the home is doing now', () => {
  it('counts a light that is on and still reporting', () => {
    const s = summarise([
      device({ id: 'lamp', category: 'light', points: power(true) }),
      device({ id: 'fan', category: 'light', points: power(false) }),
    ]);
    expect(s.lightsOn).toBe(1);
    expect(s.lightsOnLive).toBe(1);
    expect(s.silent).toBe(0);
  });

  it('does not count a light that was on when it stopped reporting', () => {
    // The bulb's last word was "on", twenty minutes before it went quiet. The rail above a grid of
    // "Offline" tiles claiming "1 on" is the contradiction a customer reads first.
    const s = summarise([
      device({ id: 'lamp', category: 'light', points: power(true), last_seen_at: minutesAgo(20) }),
    ]);
    expect(s.lightsOn).toBe(1);
    expect(s.lightsOnLive).toBe(0);
    expect(s.silent).toBe(1);
  });

  it('treats the server flag as part of the answer, not as a rival to it', () => {
    // `online` is the platform's flag and `live` is the screen's rule, but a device the platform has marked
    // offline is silent whatever its timestamp says — otherwise a stale `last_seen_at` would outvote it.
    const s = summarise([
      device({ id: 'hot', category: 'sensor', online: false }),
      device({ id: 'cold', category: 'sensor' }),
    ]);
    expect(s.online).toBe(1);
    expect(s.live).toBe(1);
    expect(s.silent).toBe(1);
  });
});
