import { BleManager, type Device } from 'react-native-ble-plx';
import { PermissionsAndroid, Platform } from 'react-native';

import {
  HAVEN_INFO,
  HAVEN_RX,
  HAVEN_SERVICE,
  HAVEN_TX,
  frame,
  parseServiceData,
  type NearbyDevice,
} from './protocol';
import type { ProvisionRequest, ProvisionResult, Radio } from './radio';

const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

function toBytes(base64: string): number[] {
  const clean = base64.replace(/=+$/, '');
  const out: number[] = [];
  let buffer = 0;
  let bits = 0;
  for (const char of clean) {
    const value = B64.indexOf(char);
    if (value < 0) continue;
    buffer = (buffer << 6) | value;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out.push((buffer >> bits) & 0xff);
    }
  }
  return out;
}

function toBase64(bytes: Uint8Array): string {
  let out = '';
  for (let i = 0; i < bytes.length; i += 3) {
    const a = bytes[i] ?? 0;
    const b = bytes[i + 1] ?? 0;
    const c = bytes[i + 2] ?? 0;
    const triple = (a << 16) | (b << 8) | c;
    out += B64.charAt((triple >> 18) & 63) + B64.charAt((triple >> 12) & 63);
    out += i + 1 < bytes.length ? B64.charAt((triple >> 6) & 63) : '=';
    out += i + 2 < bytes.length ? B64.charAt(triple & 63) : '=';
  }
  return out;
}

function toText(base64: string): string {
  return new TextDecoder().decode(new Uint8Array(toBytes(base64)));
}

/** Android 12+ splits Bluetooth and location; older versions want location only. */
async function ensurePermissions(): Promise<boolean> {
  if (Platform.OS !== 'android') return true;

  const api = Number(Platform.Version);
  const wanted =
    api >= 31
      ? [
          PermissionsAndroid.PERMISSIONS.BLUETOOTH_SCAN,
          PermissionsAndroid.PERMISSIONS.BLUETOOTH_CONNECT,
        ]
      : [PermissionsAndroid.PERMISSIONS.ACCESS_FINE_LOCATION];

  const granted = await PermissionsAndroid.requestMultiple(wanted as never);
  return wanted.every((p) => granted[p] === PermissionsAndroid.RESULTS.GRANTED);
}

export class BlePlxRadio implements Radio {
  private manager = new BleManager();

  async available(): Promise<boolean> {
    try {
      return await this.manager.state() === 'PoweredOn';
    } catch {
      return false;
    }
  }

  scan(onFound: (device: NearbyDevice) => void): () => void {
    let stopped = false;

    void (async () => {
      if (!(await ensurePermissions())) return;
      if (stopped) return;

      this.manager.startDeviceScan([HAVEN_SERVICE], null, (error, device) => {
        if (error || !device?.serviceData) return;
        const advert = parseServiceData(toBytes(device.serviceData[HAVEN_SERVICE] ?? ''));
        if (!advert) return;

        onFound({
          ...advert,
          id: device.id,
          rssi: device.rssi ?? -100,
        });
      });
    })();

    return () => {
      stopped = true;
      this.manager.stopDeviceScan();
    };
  }

  /** Wait for one `result` frame on Tx, reassembling chunks. */
  private async awaitResult(
    device: Device,
    timeoutMs: number,
  ): Promise<{ ok: boolean; code?: string; msg?: string } | null> {
    return new Promise((resolve) => {
      let buffer = '';
      let total = Number.POSITIVE_INFINITY;

      const finish = (value: { ok: boolean; code?: string; msg?: string } | null) => {
        clearTimeout(timer);
        subscription.remove();
        resolve(value);
      };

      const timer = setTimeout(() => finish(null), timeoutMs);

      const subscription = device.monitorCharacteristicForService(
        HAVEN_SERVICE,
        HAVEN_TX,
        (error, characteristic) => {
          if (error || !characteristic?.value) return;

          const bytes = toBytes(characteristic.value);
          const type = bytes[0] ?? 0;
          const header = type === 0x01 || type === 0x04 ? 5 : 3;
          if (type === 0x01 || type === 0x04) {
            total = ((bytes[3] ?? 0) << 8) | (bytes[4] ?? 0);
          }
          buffer += new TextDecoder().decode(new Uint8Array(bytes.slice(header)));

          if (buffer.length < total) return;
          try {
            const message = JSON.parse(buffer) as {
              t?: string;
              ok?: boolean;
              code?: string;
              msg?: string;
            };
            if (message.t === 'result') {
              finish({ ok: Boolean(message.ok), code: message.code, msg: message.msg });
            }
          } catch {
            finish(null);
          }
        },
      );
    });
  }

  async provision(request: ProvisionRequest): Promise<ProvisionResult> {
    try {
      const device = await this.manager.connectToDevice(request.deviceId, {
        timeout: 12_000,
      });
      await device.discoverAllServicesAndCharacteristics();

      // Read the challenge before writing: the device must be the one we think
      // it is, and the ticket must not be spendable by anyone who overheard it.
      const info = await device.readCharacteristicForService(HAVEN_SERVICE, HAVEN_INFO);
      const challenge = JSON.parse(toText(info.value ?? '')) as {
        serial: string;
        nonce: string;
      };

      const payload = JSON.stringify({
        ticket: request.ticket,
        serial: challenge.serial,
        nonce: challenge.nonce,
        ssid: request.ssid,
        password: request.password,
        endpoint: request.endpoint,
        tz: request.timezone,
      });

      // Written in order, then the device disconnects on its own when it starts
      // its Wi-Fi join — there is no ack to wait for over BLE.
      for (const chunk of frame(payload)) {
        await device.writeCharacteristicWithResponseForService(
          HAVEN_SERVICE,
          HAVEN_RX,
          toBase64(chunk),
        );
      }

      // The device sends `{"t":"result"}` once it has claimed itself. Listen for
      // it — the answer arrives in a second or two, and polling for it would be
      // a minute of guessing. If it never comes, the device is still joining:
      // report success and let the caller watch the API instead.
      const result = await this.awaitResult(device, 25_000);
      void device.cancelConnection().catch(() => undefined);

      if (result && !result.ok) {
        return { ok: false, code: result.code, message: result.msg };
      }
      return { ok: true, deviceId: request.deviceId };
    } catch (error) {
      return {
        ok: false,
        message: error instanceof Error ? error.message : 'provisioning failed',
      };
    }
  }
}
