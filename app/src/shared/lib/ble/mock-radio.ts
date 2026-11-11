import type { NearbyDevice } from './protocol';
import type { ProvisionRequest, ProvisionResult, Radio } from './radio';

/**
 * A radar with no radio.
 *
 * The pairing flow is the hardest part of the app to review, because it needs
 * hardware in a pocket. This stands in for the hardware: it advertises the two
 * seeded products, drifts their signal the way a real one drifts, and lets
 * provisioning succeed. The screens are then developed and reviewed against the
 * same code paths they will ship with.
 */
export class MockRadio implements Radio {
  async available(): Promise<boolean> {
    return true;
  }

  scan(onFound: (device: NearbyDevice) => void): () => void {
    // Hashes of hv_acme_bulb1 and hv_acme_plug1 — the same bytes the real
    // firmware computes from its product key.
    const seeds = [
      { productHash: '60576964', serialTail: '00a1', name: 'Acme Bulb', base: -58 },
      { productHash: '8ecd4857', serialTail: '00b2', name: 'Acme Smart Plug', base: -74 },
    ];

    const started = Date.now();
    const timer = setInterval(() => {
      const drift = Math.round(Math.sin((Date.now() - started) / 900) * 6);
      for (const seed of seeds) {
        onFound({
          productHash: seed.productHash,
          serialTail: seed.serialTail,
          state: 'unprovisioned',
          id: `mock-${seed.serialTail}`,
          rssi: seed.base + drift,
        });
      }
    }, 1_200);

    return () => clearInterval(timer);
  }

  async provision(request: ProvisionRequest): Promise<ProvisionResult> {
    await new Promise((resolve) => setTimeout(resolve, 1_400));

    if (!request.ssid) {
      return { ok: false, code: 'no_ssid', message: 'Choose a network first' };
    }
    if (request.password.length < 8) {
      return {
        ok: false,
        code: 'bad_password',
        message: 'WPA passwords are at least 8 characters',
      };
    }
    return { ok: true, deviceId: request.deviceId };
  }
}
