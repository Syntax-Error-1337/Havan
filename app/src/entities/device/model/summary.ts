import { freshness } from './silence';
import type { Device } from './types';

/**
 * What a home is doing, computed from what the devices actually report.
 *
 * Every figure here is derived: there is no "home state" row on the server to drift out of date, and
 * no screen-local tally that disagrees with another screen. A tile that says "3 lights on" and a
 * header that says "2 lights on" is the classic version of this bug, and deriving both from the same
 * function is the cheap fix.
 */
export type HomeSummary = {
  devices: number;
  online: number;
  /**
   * Devices still reporting, by the same rule the tiles use (`freshness`). `online` is the server's flag and
   * can disagree with it; this is the number the screen can defend, and the reason a rail can say
   * "3 lights on" while the header says "1 of 6 online".
   */
  live: number;
  /** Devices not live: their values are the last thing they said, not the current state. */
  silent: number;
  lightsOn: number;
  /** Lights on *and* still reporting — the only count that is true of the house right now. */
  lightsOnLive: number;
  lights: number;
  /** The room reading, when some device actually measures one. */
  temperature: { value: number; unit: string } | null;
  humidity: { value: number; unit: string } | null;
  /** A target range across climate devices, when there is more than one. */
  climateRange: { low: number; high: number; unit: string } | null;
  openCovers: number;
  unlocked: number;
};

function num(points: Device['points'], capability: string): { value: number; unit: string } | null {
  const point = points.find(
    (p) => p.capability === capability && typeof p.value === 'number',
  );
  if (!point || typeof point.value !== 'number') return null;
  return { value: point.value, unit: point.unit ?? '' };
}

export function summarise(devices: Device[]): HomeSummary {
  const lights = devices.filter((d) => d.category === 'light');
  const climate = devices.filter((d) => d.category === 'climate' || d.category === 'thermostat');

  const targets = climate
    .map((d) => num(d.points, 'target_temperature'))
    .filter((t): t is { value: number; unit: string } => t !== null);

  const roomTemp =
    devices.map((d) => num(d.points, 'temperature')).find((t) => t !== null) ?? null;
  const roomHumidity =
    devices.map((d) => num(d.points, 'humidity')).find((h) => h !== null) ?? null;

  const isOn = (d: Device): boolean => {
    const power = d.points.find((p) => p.capability === 'switch');
    const level = d.points.find((p) => p.capability === 'brightness');
    if (power) return power.value === true;
    return typeof level?.value === 'number' && level.value > 0;
  };
  const liveCount = devices.filter((d) => freshness(d) === 'live').length;

  return {
    devices: devices.length,
    online: devices.filter((d) => d.online).length,
    live: liveCount,
    silent: devices.length - liveCount,
    lights: lights.length,
    lightsOn: lights.filter(isOn).length,
    lightsOnLive: lights.filter((d) => isOn(d) && freshness(d) === 'live').length,
    temperature: roomTemp,
    humidity: roomHumidity,
    climateRange:
      targets.length > 1
        ? {
            low: Math.min(...targets.map((t) => t.value)),
            high: Math.max(...targets.map((t) => t.value)),
            unit: targets[0]?.unit ?? '',
          }
        : null,
    openCovers: devices.filter((d) => {
      const position = d.points.find((p) => p.capability === 'cover');
      return typeof position?.value === 'number' && position.value > 0;
    }).length,
    unlocked: devices.filter((d) => {
      const lock = d.points.find((p) => p.capability === 'lock');
      return lock?.value === false;
    }).length,
  };
}
