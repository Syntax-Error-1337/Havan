import { fireEvent, render } from '@testing-library/react-native';

import { ThemeProvider } from '../../app-shell/theme';
import { NO_DEVICE_CHECKS, NoDeviceHelp } from './NoDeviceHelp';

/**
 * The screen's answer to "it is not appearing".
 *
 * Two things are worth a test here, and neither is layout: that the list is **collapsed** by default (seven paragraphs
 * in front of a scan that is working is noise), and that every check the platform actually enforces is present — the
 * 2.4 GHz radio, WPA2-PSK AES, and a device that was paired before never announces itself again. Those are the three
 * causes this hardware has, and a help list that omits them is a paragraph of encouragement.
 */
describe('the help for a device that does not appear', () => {
  // `render` returns a promise in this version of RNTL: without the await, every query fails for the wrong reason.
  const renderHelp = async () =>
    render(
      <ThemeProvider>
        <NoDeviceHelp hint="Nothing yet — these are the things worth checking." />
      </ThemeProvider>,
    );

  it('is collapsed until it is asked for', async () => {
    const view = await renderHelp();
    expect(await view.findByText('The device is not appearing')).toBeTruthy();
    expect(view.queryByText(NO_DEVICE_CHECKS[0]!.title)).toBeNull();

    await fireEvent.press(view.getByTestId('no-device-help'));
    expect(await view.findByText(NO_DEVICE_CHECKS[0]!.title)).toBeTruthy();
  });

  it('names the three limits this hardware has, not generic advice', () => {
    const text = NO_DEVICE_CHECKS.map((c) => `${c.title} ${c.body}`).join(' ').toLowerCase();
    // ESP-12E: no 5 GHz radio.
    expect(text).toContain('2.4 ghz');
    // Mixed WPA/WPA2 and WPA3-only are refused by the radio.
    expect(text).toContain('wpa2-psk');
    expect(text).toContain('aes');
    // A claimed device leaves the pairing mode for good, which is the cause people never suspect.
    expect(text).toContain('already belongs to a home');
  });

  it('says what to do, not just what is wrong', () => {
    // Every check ends in an action the customer can take: a verb, in the second person, in the first sentence.
    for (const check of NO_DEVICE_CHECKS) {
      expect(check.body.length).toBeGreaterThan(40);
      expect(check.body).toMatch(/^(Unplug|A device|This hardware|Mixed|Guest|Pairing|The scan)/);
    }
  });
});
