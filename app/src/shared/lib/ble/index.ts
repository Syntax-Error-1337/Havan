import { MockRadio } from './mock-radio';
import { BlePlxRadio } from './plx-radio';
import type { Radio } from './radio';

export * from './protocol';
export type { ProvisionRequest, ProvisionResult, Radio } from './radio';

/**
 * One radio per process. The choice is made here and nowhere else, so no screen
 * has to know whether it is talking to hardware or to the stand-in.
 *
 * `EXPO_PUBLIC_BLE_MOCK=1` forces the mock: the simulator has no radio, and CI
 * has neither hardware nor permission prompts.
 */
export const radio: Radio =
  process.env.EXPO_PUBLIC_BLE_MOCK === '1' ? new MockRadio() : new BlePlxRadio();
