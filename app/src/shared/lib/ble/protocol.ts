/**
 * The radar's vocabulary, and the one place the BLE wire format is decoded.
 *
 * A device advertises four bytes of product hash, four of serial tail and one
 * state byte. That is deliberately tiny: the phone draws a name and an icon
 * before it connects, using the catalogue it already has.
 */

export const HAVEN_SERVICE = '5a1d0001-8f2b-4c6e-9d1a-7c3e5f9b2a10';
export const HAVEN_INFO = '5a1d0002-8f2b-4c6e-9d1a-7c3e5f9b2a10';
export const HAVEN_RX = '5a1d0003-8f2b-4c6e-9d1a-7c3e5f9b2a10';
export const HAVEN_TX = '5a1d0004-8f2b-4c6e-9d1a-7c3e5f9b2a10';

export type DeviceState = 'unprovisioned' | 'provisioning' | 'needs_repair' | 'claimed';

export type Advert = {
  /** 8 hex characters: the first four bytes of SHA-256(product_key). */
  productHash: string;
  /** Last four bytes of the serial, for telling twenty identical boxes apart. */
  serialTail: string;
  state: DeviceState;
};

export type NearbyDevice = Advert & {
  id: string;
  rssi: number;
  /** Filled in from the catalogue; absent until the lookup returns. */
  name?: string;
  category?: string;
};

const STATE_BYTE: DeviceState[] = [
  'unprovisioned',
  'provisioning',
  'needs_repair',
  'claimed',
];

/**
 * Decode the advertising service data. Returns `null` for anything that is not
 * ours — a neighbour's fitness band must not appear on the radar.
 */
export function parseServiceData(bytes: number[]): Advert | null {
  if (bytes.length < 9) return null;

  const hex = (slice: number[]) => slice.map((b) => b.toString(16).padStart(2, '0')).join('');
  const productHash = hex(bytes.slice(0, 4));
  const serialTail = hex(bytes.slice(4, 8));
  const state = STATE_BYTE[(bytes[8] ?? 1) - 1];

  if (!state) return null;
  return { productHash, serialTail, state };
}

/**
 * The advertised tail is the last four bytes of the serial, hex-encoded. Turn
 * it back into text to match it against a serial the API returned: comparing
 * hex to text never matches, and that failure looks like "the device never
 * arrived".
 */
export function serialTailMatches(serial: string, tailHex: string): boolean {
  const text = tailHex
    .match(/.{2}/g)
    ?.map((pair) => String.fromCharCode(Number.parseInt(pair, 16)))
    .join('');
  if (!text) return false;
  return serial.toUpperCase().endsWith(text.toUpperCase());
}

/** RSSI to a 0–4 bar count. The thresholds are the ones that felt right on a phone. */
export function signalBars(rssi: number): number {
  if (rssi >= -55) return 4;
  if (rssi >= -67) return 3;
  if (rssi >= -78) return 2;
  if (rssi >= -90) return 1;
  return 0;
}

/**
 * Frame a JSON message for the Rx characteristic.
 *
 * One message may exceed an ATT payload, so it is chunked: type, message id,
 * total length on the first chunk, then the bytes. The device reassembles in
 * order; a dropped chunk times out on its side.
 */
export function frame(payload: string, chunkSize = 180): Uint8Array[] {
  const bytes = new TextEncoder().encode(payload);
  const id = Math.floor(Math.random() * 0xffff);
  const frames: Uint8Array[] = [];

  let offset = 0;
  while (offset < bytes.length) {
    const first = offset === 0;
    const chunk = bytes.slice(offset, offset + chunkSize);
    const last = offset + chunk.length >= bytes.length;

    const header = new Uint8Array(first ? 5 : 3);
    header[0] = first ? (last ? 0x04 : 0x01) : last ? 0x03 : 0x02;
    header[1] = (id >> 8) & 0xff;
    header[2] = id & 0xff;
    if (first) {
      header[3] = (bytes.length >> 8) & 0xff;
      header[4] = bytes.length & 0xff;
    }

    const out = new Uint8Array(header.length + chunk.length);
    out.set(header, 0);
    out.set(chunk, header.length);
    frames.push(out);
    offset += chunk.length;
  }

  return frames;
}
