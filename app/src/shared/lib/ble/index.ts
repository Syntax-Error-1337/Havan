import { MockRadio } from './mock-radio';
import { NullRadio, unsupportedReason } from './null-radio';
import type { Radio } from './radio';

export * from './protocol';
export type { ProvisionRequest, ProvisionResult, Radio } from './radio';

let chosen: Radio | null = null;

/**
 * One radio per process, created on first use — not at import.
 *
 * The difference matters on web: `react-native-ble-plx` throws while being
 * *constructed* when the native module is missing, so a module-scope singleton took
 * the whole app down the moment the router imported the radar route. Nothing above
 * a screen should touch the radio until a screen asks for it.
 *
 * Precedence: the mock when `EXPO_PUBLIC_BLE_MOCK=1` (simulator, CI), the real
 * radio on a phone, and a no-op that reports "unsupported" anywhere the native
 * module cannot load.
 */
export function getRadio(): Radio {
  if (chosen) return chosen;

  if (process.env.EXPO_PUBLIC_BLE_MOCK === '1') {
    chosen = new MockRadio();
    return chosen;
  }

  try {
    // Required lazily on purpose: the import itself is what throws on web.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { BlePlxRadio } = require('./plx-radio') as typeof import('./plx-radio');
    chosen = new BlePlxRadio();
  } catch {
    chosen = new NullRadio(unsupportedReason());
  }

  return chosen;
}

/** Tests need a clean slate; nothing else should call this. */
export function resetRadio(): void {
  chosen = null;
}
