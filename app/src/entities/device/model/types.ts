/** Wire types. Generated from OpenAPI once the API emits it; hand-written until then. */

export type PointKind = 'bool' | 'int' | 'float' | 'text' | 'enum' | 'raw';

export type DevicePoint = {
  id: string;
  code: string;
  capability: string;
  slot: string;
  kind: PointKind;
  writable: boolean;
  /** `off | change | interval:N | all` — whether the platform stores this point. */
  history?: string | null;
  unit?: string | null;
  min?: number | null;
  max?: number | null;
  value: boolean | number | string | null;
  updated_at?: string | null;
};

export type Device = {
  id: string;
  home_id?: string | null;
  /** Printed on the box and advertised over BLE — how a radar finds it again. */
  serial: string;
  name: string;
  room_id?: string | null;
  room_name?: string | null;
  product_key: string;
  category: string;
  online: boolean;
  fw_version?: string | null;
  last_seen_at?: string | null;
  points: DevicePoint[];
};

/** The primary control the dashboard renders, in priority order. */
export const PRIMARY_POINT_ORDER = ['switch.power', 'lock.locked', 'cover.position'] as const;

export function primaryPoint(device: Device): DevicePoint | undefined {
  for (const code of PRIMARY_POINT_ORDER) {
    const p = device.points.find((x) => x.code === code);
    if (p) return p;
  }
  // A sensor has nothing writable and no switch: its reading *is* its primary point, and without
  // this fallback a thermometer tile renders an em dash where the temperature should be.
  return (
    device.points.find((p) => p.writable) ??
    // Battery first is alphabetical, not meaningful: a sensor tile that leads with 87 % is answering
    // a question nobody asked. Prefer the measurement, then anything.
    // Then by what a person checks first. Alphabetical order put `humidity` before `temperature`,
    // so a climate sensor led with the reading nobody opened the app for.
    ['temperature', 'humidity', 'energy', 'power']
      .map((capability) => device.points.find((p) => p.capability === capability))
      .find((p) => p !== undefined) ??
    device.points.find((p) => p.capability !== 'battery' && p.value !== null && p.value !== undefined) ??
    device.points.find((p) => p.value !== null && p.value !== undefined)
  );
}
