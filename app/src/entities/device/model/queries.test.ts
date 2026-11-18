import { patchDeviceIn, withPointValue } from './queries';
import type { Device } from './types';

const device = {
  id: 'd1',
  home_id: 'h1',
  name: 'Floor Lamp',
  category: 'light',
  online: true,
  points: [
    { code: 'switch.power', kind: 'bool', writable: true, value: false, unit: null },
    { code: 'brightness.level', kind: 'int', writable: true, value: 40, unit: '%' },
  ],
} as unknown as Device;

describe('patchDeviceIn', () => {
  it('patches the plain device list', () => {
    const list = [device, { ...device, id: 'd2' }];
    const next = patchDeviceIn(list, 'd1', 'switch.power', true) as typeof list;
    expect(next[0]!.points.find((p) => p.code === 'switch.power')?.value).toBe(true);
    expect(next[1]).toBe(list[1]);
  });

  it('patches the dashboard aggregate, which is what the tiles are drawn from', () => {
    // The bug: only arrays were handled, so the optimistic write landed in a cache nothing rendered.
    const dashboard = { devices: [device], presence: { total: 1, online: 1, silent: 0 } };
    const next = patchDeviceIn(dashboard, 'd1', 'switch.power', true) as typeof dashboard;
    expect(next.devices[0]!.points.find((p) => p.code === 'switch.power')?.value).toBe(true);
    expect(next.presence).toEqual(dashboard.presence);
  });

  it('returns the value untouched when the device is not in it, so the caller writes nothing', () => {
    const list = [{ ...device, id: 'd2' }];
    expect(patchDeviceIn(list, 'd1', 'switch.power', true)).toBe(list);
    expect(patchDeviceIn({ items: [] }, 'd1', 'switch.power', true)).toEqual({ items: [] });
  });
});

describe('withPointValue', () => {
  it('moves the one point it was asked to move', () => {
    const next = withPointValue(device, 'switch.power', true);
    expect(next.points.find((p) => p.code === 'switch.power')?.value).toBe(true);
    expect(next.points.find((p) => p.code === 'brightness.level')?.value).toBe(40);
  });

  it('returns a new device rather than mutating the cached one', () => {
    const next = withPointValue(device, 'switch.power', true);
    expect(next).not.toBe(device);
    // The list cache still holds the old object; a mutation in place would change it for every screen at once
    // and make the roll-back in `onError` a no-op.
    expect(device.points.find((p) => p.code === 'switch.power')?.value).toBe(false);
  });

  it('leaves a device alone when the code is not one of its points', () => {
    const next = withPointValue(device, 'colour.hue', 120);
    expect(next.points).toHaveLength(2);
    expect(next.points).toEqual(device.points);
  });
});
