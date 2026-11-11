import type { NearbyDevice } from './protocol';

export type ProvisionRequest = {
  deviceId: string;
  ssid: string;
  password: string;
  ticket: string;
  endpoint: string;
  timezone: string;
};

export type ProvisionResult = {
  ok: boolean;
  deviceId?: string;
  code?: string;
  message?: string;
};

/**
 * The radio, as the app needs it. Two implementations exist: `BlePlxRadio`
 * for hardware and `MockRadio` for development, CI and the simulator — the UI
 * cannot tell them apart, which is what lets the pairing flow be built and
 * reviewed before firmware exists.
 */
export type Radio = {
  /** True when this platform can scan at all (web cannot). */
  available(): Promise<boolean>;
  /** Start scanning. Returns a stop function. */
  scan(onFound: (device: NearbyDevice) => void): () => void;
  provision(request: ProvisionRequest): Promise<ProvisionResult>;
};
