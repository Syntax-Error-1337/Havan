import { useState } from 'react';
import { Pressable, View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText, Card, Icon } from '../../shared/ui';

/**
 * What to do when the device does not appear.
 *
 * This screen's only previous answer to the most common first-run failure was the sentence "Nothing yet. Devices show up
 * within a few seconds." — which, after a minute, stops being encouragement and starts being wrong. Every cause below is
 * something this platform actually does, in the order a person is likely to hit it: the board is 2.4 GHz only with
 * WPA2-PSK AES, a device that was paired before never announces itself again, and a guest network with client isolation
 * drops the very packet the phone is waiting for.
 *
 * Deliberately a list of *checks*, not of guarantees: we cannot see the customer's router, so the honest thing is to
 * name what to look for rather than promise that one of these is the answer.
 */
export const NO_DEVICE_CHECKS: { title: string; body: string }[] = [
  {
    title: 'Power-cycle the device',
    body: 'Unplug it for five seconds and plug it back in. It announces itself only for the first few minutes after it starts, and only until somebody pairs it.',
  },
  {
    title: 'Was it paired before?',
    body: 'A device that already belongs to a home stops advertising. Reset it — hold its button until the light blinks fast — so it looks for a new home.',
  },
  {
    title: 'The network must be 2.4 GHz',
    body: 'This hardware has no 5 GHz radio, and a router that steers a single network name between the two bands can hide it. Give the 2.4 GHz band its own name for pairing.',
  },
  {
    title: 'WPA2-PSK with AES',
    body: 'Mixed WPA/WPA2, WPA3-only and enterprise networks are refused by the radio. If the router offers WPA2/WPA3 mixed, choose WPA2-PSK (AES) for the 2.4 GHz band.',
  },
  {
    title: 'Not a guest network',
    body: 'Guest networks usually isolate clients from each other, and the phone will never see the device announce itself across that wall.',
  },
  {
    title: 'Within two metres',
    body: 'Pairing is over Bluetooth, which is short-ranged by design: two metres, one wall at most, and away from a microwave or a USB 3 hub.',
  },
  {
    title: 'Bluetooth and Location are on',
    body: 'The scan needs the Bluetooth permission, and on Android a nearby-devices permission too. Settings on the phone, not here.',
  },
];

/**
 * The list, behind a quiet disclosure.
 *
 * Collapsed by default: while a scan is working, seven paragraphs of advice is noise in front of a number that is
 * counting devices. Expanded, it is the only thing on the screen worth reading.
 */
export function NoDeviceHelp({ hint }: { hint?: string }) {
  const { color } = useTheme();
  const [open, setOpen] = useState(false);

  return (
    <View style={{ gap: tokens.space.sm }}>
      <Pressable
        onPress={() => setOpen((v) => !v)}
        accessibilityRole="button"
        accessibilityState={{ expanded: open }}
        accessibilityLabel="The device is not appearing"
        testID="no-device-help"
        style={{
          minHeight: tokens.touch.min,
          flexDirection: 'row',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: tokens.space.sm,
        }}
      >
        <View style={{ gap: 2, flex: 1 }}>
          <AppText weight="600">The device is not appearing</AppText>
          {hint ? (
            <AppText variant="tiny" tone="muted">
              {hint}
            </AppText>
          ) : null}
        </View>
        {/* The one chevron the icon set has, turned: a second glyph for "up" would be a second drawing of the same
            idea, and the set is deliberately small. */}
        {/* Right-pointing by default: −90° turns it up when open, +90° turns it down when closed. The first version
            had the two the wrong way round, so the chevron pointed up at rest and down once the list was showing. */}
        <View style={{ transform: [{ rotate: open ? '-90deg' : '90deg' }] }}>
          <Icon name="chevron-right" size={16} color={color.muted} />
        </View>
      </Pressable>

      {open
        ? NO_DEVICE_CHECKS.map((check, index) => (
            <Card key={check.title} padded>
              <View style={{ flexDirection: 'row', gap: tokens.space.md, alignItems: 'flex-start' }}>
                {/* Numbers, because this is a sequence of things to try, not a list of facts. */}
                <View
                  style={{
                    width: 22,
                    height: 22,
                    borderRadius: 11,
                    alignItems: 'center',
                    justifyContent: 'center',
                    backgroundColor: color.elevated,
                    marginTop: 1,
                  }}
                >
                  <AppText variant="tiny" weight="700" tone="muted">
                    {index + 1}
                  </AppText>
                </View>
                <View style={{ flex: 1, gap: 2 }}>
                  <AppText weight="600">{check.title}</AppText>
                  <AppText variant="caption" tone="muted">
                    {check.body}
                  </AppText>
                </View>
              </View>
            </Card>
          ))
        : null}
    </View>
  );
}
