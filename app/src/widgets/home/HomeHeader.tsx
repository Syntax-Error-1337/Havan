import { Pressable, StyleSheet, View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText, Glass, Icon, type IconName } from '../../shared/ui';
import { withAlpha } from '../../shared/ui/Glass';
import { accentFor } from '../../shared/config/tokens';
import type { HomeSummary } from '../../entities/device/model/summary';

/**
 * The top of the screen: who is looking, which home, and what time it is there.
 *
 * The avatar carries the person's initials rather than a photo because the platform never asks for
 * one — a placeholder face would be a lie about a feature. The weather slot the reference screens
 * have is deliberately absent: this platform knows the temperature inside the house, not outside it,
 * and a made-up sky would be the first thing a customer noticed.
 */
export function HomeHeader({
  name,
  person,
  subtitle,
  online,
  total,
  silent,
  unknown = false,
  unread = 0,
  onOpenAlerts,
  onOpenHomes,
}: {
  name: string;
  person: string;
  subtitle: string;
  /** Devices the home can hear from, and how many it has. The pill is the answer to "is my house up?" */
  online: number;
  total: number;
  /**
   * The counts are not known — the request failed. `0` is a number and "no devices" is a sentence, and
   * neither is true when the hub could not be asked.
   */
  unknown?: boolean;
  /** Devices not reporting: the pill turns amber and says so, instead of printing a green number it doubts. */
  silent: number;
  /** Unread alerts. A status belongs in the header; as a pill beside "New room" it read as a filter. */
  unread?: number;
  onOpenAlerts?: () => void;
  /** Opens the home switcher. An account with one home still needs the way to add a second. */
  onOpenHomes?: () => void;
}) {
  const { color } = useTheme();
  const initials = person
    .split(/[\s@.]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? '')
    .join('');

  return (
    <View style={{ gap: tokens.space.md }}>
      <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
        <View
          style={{
            width: 42,
            height: 42,
            borderRadius: 21,
            alignItems: 'center',
            justifyContent: 'center',
            backgroundColor: withAlpha(color.accent, 0.22),
            borderWidth: 1,
            borderColor: withAlpha(color.accent, 0.42),
          }}
        >
          <AppText variant="caption" weight="600" style={{ color: color.accent }}>
            {initials || 'H'}
          </AppText>
        </View>

        <Pressable
          accessibilityRole="button"
          accessibilityLabel={`${name} — switch home`}
          onPress={onOpenHomes}
          // The block is 42 points tall (an 11-point eyebrow over a 22-point title), and 42 is not 44.
          style={{ flex: 1, gap: 1, minHeight: 44, justifyContent: 'center' }}
        >
          <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.2 }}>
            DASHBOARD
          </AppText>
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: 4 }}>
            <AppText variant="title" weight="700" numberOfLines={1} style={{ flexShrink: 1 }}>
              {name}
            </AppText>
            <Icon name="chevron-right" size={16} color={color.muted} />
          </View>
        </Pressable>

        {onOpenAlerts ? (
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={unread > 0 ? `Alerts, ${unread} unread` : 'Alerts'}
            onPress={onOpenAlerts}
            style={{
              // 44, not 38: a 38-point circle is 6 points under the minimum a thumb can reliably hit, and
              // the bell is the one control in this corner.
              width: 44,
              height: 44,
              borderRadius: 22,
              alignItems: 'center',
              justifyContent: 'center',
              backgroundColor: color.glass,
              borderWidth: StyleSheet.hairlineWidth,
              // One orange thing on the right, not two. An orange ring around an orange badge beside an
              // amber pill made the corner a clump; the badge is enough to say "there is something here".
              borderColor: color.border,
            }}
          >
            <Icon name="bell" size={19} color={color.text} />
            {unread > 0 ? (
              <View
                style={{
                  position: 'absolute',
                  top: 2,
                  right: 2,
                  minWidth: 15,
                  height: 15,
                  borderRadius: 8,
                  paddingHorizontal: 3,
                  alignItems: 'center',
                  justifyContent: 'center',
                  backgroundColor: color.warning,
                }}
              >
                <AppText style={{ fontSize: 9, lineHeight: 11, fontWeight: '700', color: color.bg }}>
                  {unread > 9 ? '9+' : unread}
                </AppText>
              </View>
            ) : null}
          </Pressable>
        ) : null}

        {/* This pill was a clock with a lightning bolt beside it: a redundant time (the OS draws one in the
            status bar) under a glyph that means power, in warning orange, on a healthy house. It is now the
            one question the header can answer that the clock could not — is anything not reporting? */}
        {/* Nothing to be present. "no devices" beside "0 devices · 0 rooms" and an empty-state heading
            is the same fact three times, and a pill is the most expensive of the three. */}
        {total === 0 || unknown ? null : (
        <Glass
          padded={false}
          style={{ paddingHorizontal: 12, paddingVertical: 8 }}
          testID="presence-pill"
        >
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: 6 }}>
            <View
              style={{
                width: 7,
                height: 7,
                borderRadius: 4,
                // Green means "everything is reporting". A home with nothing in it is not healthy — it is
                // empty, and a green dot beside "no devices" reads as a device that is fine.
                backgroundColor:
                  total === 0 || unknown
                    ? color.muted
                    : silent === 0
                      ? color.success
                      : online === 0
                        ? color.danger
                        : color.warning,
              }}
            />
            {/* The number lives here and the *explanation* lives in the banner below, so the same count is
                not printed twice in two different vocabularies ("1/6 online" / "5 not reporting"). */}
            <AppText variant="caption" weight="600">
              {/* Locale separators: "1200 offline" is a number, "1,200 offline" is a count — the same
                  reason the subtitle underneath formats its total. */}
              {unknown
                ? 'unknown'
                : total === 0
                  ? 'no devices'
                  : silent === 0
                    ? `${online.toLocaleString()} of ${total.toLocaleString()} online`
                    : `${silent.toLocaleString()} offline`}
            </AppText>
          </View>
        </Glass>
        )}
      </View>

      <AppText variant="caption" tone="muted">
        {subtitle}
      </AppText>
    </View>
  );
}

/** The rail of home-level figures: the three numbers a person checks without opening anything. */
export function StatRail({
  summary,
  rooms,
  showTemperature = false,
}: {
  summary: HomeSummary;
  rooms: number;
  /** True when the hero card is already showing the room reading. */
  showTemperature?: boolean;
}) {
  const { color, dark } = useTheme();
  // The rail used to hard-code its hues, which in daylight put a 1.6:1 cyan on a white card. It asks
  // the same category palette every tile does now, so a colour change lands everywhere at once.
  const tint = (category: string, fallback: string) => accentFor(category, fallback, dark);

  const chips: { icon: IconName; label: string; value: string; accent: string }[] = [];

  if (summary.lights > 0) {
    // Lights that are on *and still reporting*. A bulb that was on when it lost the network is not a light
    // that is on now, and the rail above a grid of "Offline" tiles claiming "3 on" is the contradiction a
    // customer reads first. When something is silent the banner under the header says why the numbers moved.
    chips.push({
      icon: 'bulb',
      label: 'Lights',
      value: `${summary.lightsOnLive} on`,
      accent: accentFor('light', color.accent),
    });
  }
  // "Inside" only appears when the hero is not already showing that same reading: two places on one
  // screen with the same number, labelled differently, is how a dashboard starts to feel untrustworthy.
  if (summary.temperature && !showTemperature) {
    chips.push({
      icon: 'climate',
      label: 'Inside',
      value: `${summary.temperature.value}${summary.temperature.unit || '°'}`,
      accent: accentFor('climate', color.accent),
    });
  }
  if (summary.humidity) {
    chips.push({
      icon: 'drop',
      label: 'Humidity',
      value: `${summary.humidity.value}${summary.humidity.unit || '%'}`,
      accent: accentFor('sensor', color.accent),
    });
  }
  // No "Rooms" chip: the header already says "6 devices · 3 rooms", and a rail that repeats the subtitle
  // in a card is the cheapest way to look busy. `rooms` is still counted by the caller for that line.
  if (summary.unlocked > 0) {
    chips.push({
      icon: 'lock',
      label: 'Unlocked',
      value: String(summary.unlocked),
      accent: accentFor('lock', color.accent),
    });
  }

  if (chips.length === 0) return null;

  return (
    <View style={{ flexDirection: 'row', gap: 8 }}>
      {chips.map((chip) => (
        <Glass key={chip.label} style={{ flex: 1 }} accent={chip.accent}>
          <View style={{ gap: 6 }}>
            <Icon name={chip.icon} size={16} color={chip.accent} />
            <AppText variant="tiny" tone="muted">
              {chip.label}
            </AppText>
            <AppText variant="subhead" weight="700" numberOfLines={1}>
              {chip.value}
            </AppText>
          </View>
        </Glass>
      ))}
      {chips.length === 0 ? <View style={{ height: 0 }} /> : null}
    </View>
  );
}
