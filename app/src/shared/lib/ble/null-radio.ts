import { Platform } from 'react-native';

import type { NearbyDevice } from './protocol';

/**
 * The radio, when there is no radio.
 *
 * A web build has no Bluetooth module, and `react-native-ble-plx` throws on import
 * there — not when you scan, when you *load* it. Constructing that class at module
 * scope took the whole app down on web, because a router imports every route
 * eagerly. So the choice of radio is lazy, and this is what it falls back to.
 *
 * `available()` returns false rather than throwing, which the radar already
 * renders as "this device can't scan" instead of an empty list.
 */
export class NullRadio {
  readonly reason: string;

  constructor(reason: string) {
    this.reason = reason;
  }

  async available(): Promise<boolean> {
    return false;
  }

  scan(_onFound: (device: NearbyDevice) => void): () => void {
    return () => {};
  }

  async provision(): Promise<{ ok: false; code: string; message: string }> {
    return { ok: false, code: 'unsupported', message: this.reason };
  }
}

export function unsupportedReason(): string {
  return Platform.OS === 'web'
    ? 'Pairing needs the phone app: a browser has no Bluetooth.'
    : 'Bluetooth is unavailable on this device.';
}
