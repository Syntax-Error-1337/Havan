import { View } from 'react-native';

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
  clock,
}: {
  name: string;
  person: string;
  subtitle: string;
  clock: string;
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

        <View style={{ flex: 1 }}>
          <AppText variant="tiny" tone="muted" style={{ letterSpacing: 1.2 }}>
            DASHBOARD
          </AppText>
          <AppText variant="title" weight="700" numberOfLines={1}>
            {name}
          </AppText>
        </View>

        <Glass padded={false} style={{ paddingHorizontal: 12, paddingVertical: 8 }}>
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: 6 }}>
            <Icon name="energy" size={14} color={color.warning} />
            <AppText variant="caption" weight="600">
              {clock}
            </AppText>
          </View>
        </Glass>
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
    chips.push({
      icon: 'bulb',
      label: 'Lights',
      value: `${summary.lightsOn} on`,
      accent: tint('light', '#8B5CF6'),
    });
  }
  // "Inside" only appears when the hero is not already showing that same reading: two places on one
  // screen with the same number, labelled differently, is how a dashboard starts to feel untrustworthy.
  if (summary.temperature && !showTemperature) {
    chips.push({
      icon: 'climate',
      label: 'Inside',
      value: `${summary.temperature.value}${summary.temperature.unit || '°'}`,
      accent: tint('climate', '#FB923C'),
    });
  }
  if (summary.humidity) {
    chips.push({
      icon: 'drop',
      label: 'Humidity',
      value: `${summary.humidity.value}${summary.humidity.unit || '%'}`,
      accent: tint('sensor', '#22D3EE'),
    });
  }
  if (rooms > 0) {
    chips.push({ icon: 'home', label: 'Rooms', value: String(rooms), accent: color.accentAlt });
  }
  if (summary.unlocked > 0) {
    chips.push({
      icon: 'lock',
      label: 'Unlocked',
      value: String(summary.unlocked),
      accent: tint('lock', '#F472B6'),
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
