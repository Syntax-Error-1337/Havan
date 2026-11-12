import { NullRadio, unsupportedReason } from './null-radio';
import { getRadio, resetRadio } from './index';

/**
 * The fallback radio: a browser and a simulator have no Bluetooth module, and the
 * native one throws while being *constructed* — not while scanning. That throw at
 * import time is what used to blank the whole app on web.
 */
describe('the radio a platform without Bluetooth gets', () => {
  it('reports that it cannot scan, instead of throwing', async () => {
    const radio = new NullRadio(unsupportedReason());
    await expect(radio.available()).resolves.toBe(false);
  });

  it('scans nothing and can be stopped', () => {
    const radio = new NullRadio('no radio');
    const stop = radio.scan(() => {
      throw new Error('a null radio must never report a device');
    });
    expect(typeof stop).toBe('function');
    stop();
  });

  it('refuses to provision with a reason a screen can show', async () => {
    const result = await new NullRadio('a browser has no Bluetooth').provision();
    expect(result.ok).toBe(false);
    expect(result.message).toBe('a browser has no Bluetooth');
  });

  it('is created once, on first use', () => {
    resetRadio();
    expect(getRadio()).toBe(getRadio());
  });
});
