import { View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText, AreaChart, Glass, Icon, iconForCapability } from '../../shared/ui';
import { withAlpha } from '../../shared/ui/Glass';
import { accentFor, glowFor } from '../../shared/config/tokens';
import {
  freshness,
  silenceWords,
  useHistory,
  useSetPoint,
  type Device,
} from '../../entities/device';

/**
 * The one device the screen leads with.
 *
 * A home screen with a grid of equal tiles makes you read every tile to find the thing you opened the
 * app for. One card carries the reading that matters — the room temperature, the light level, the
 * lock — with its last day drawn under it, and the grid below is for everything else.
 *
 * The device is chosen from data (first online device with a numeric primary point), not from a
 * setting nobody has filled in. When a home has no such device the card simply does not render;
 * inventing a figure would be worse than the empty space.
 */
export function HeroCard({ device }: { device: Device }) {
  const { color, dark } = useTheme();
  const accent = accentFor(device.category, color.accent, dark);
  const glow = glowFor(device.category, color.glow);

  // Points arrive sorted by code, so "first numeric" is `battery.level` — a percentage nobody
  // opened the app for. Preference, not order: what a person checks first, then what they can read.
  const preferred = ['temperature', 'humidity', 'energy', 'brightness', 'power', 'battery'];
  const numeric = device.points.filter((p) => typeof p.value === 'number');
  const point =
    preferred
      .map((capability) => numeric.find((p) => p.capability === capability))
      .find((p) => p !== undefined) ?? numeric[0];
  const { data: history } = useHistory(device.id, point?.code, '1d');
  const setPoint = useSetPoint(device.id);

  const value = typeof point?.value === 'number' ? point.value : null;
  const state = freshness(device);

  // The chart's own scale, in words: a line with no numbers on it answers "did it move" and
  // nothing else. Min and max are the two facts a home owner actually wants from a day of readings.
  const readings = (history?.buckets?.map((b) => b.v) ?? []).concat(value === null ? [] : [value]);
  // The current reading is part of the day. Leaving it out made the card contradict itself — a
  // number above its own maximum — which is exactly the kind of thing a customer notices first.
  const rangeText =
    readings.length > 1
      ? `${Math.min(...readings).toFixed(1)}–${Math.max(...readings).toFixed(1)} ${point?.unit ?? ''} · last 24 h`
      : 'last 24 h';
  const isSwitchable = device.points.some((p) => p.capability === 'switch' && p.writable);

  return (
    <Glass
      accent={accent}
      glow={state === 'live' ? glow : undefined}
      // Three levels, not two: a device nobody has heard from for four hours should not look as alive
      // as one that reported a minute ago, and should not look as dead as one that is gone.
      style={{ padding: 18, opacity: state === 'live' ? 1 : state === 'stale' ? 0.88 : 0.72 }}
    >
      <View style={{ gap: 14 }}>
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: 10 }}>
          <View
            style={{
              width: 34,
              height: 34,
              borderRadius: 17,
              alignItems: 'center',
              justifyContent: 'center',
              backgroundColor: withAlpha(accent, dark ? 0.24 : 0.14),
            }}
          >
            <Icon name={iconForCapability(point?.capability)} size={17} color={accent} />
          </View>
          <View style={{ flex: 1 }}>
            <AppText variant="caption" weight="600" numberOfLines={1}>
              {device.name}
            </AppText>
            <AppText variant="tiny" tone="muted" numberOfLines={1}>
              {device.room_name ?? 'No room'} ·{' '}
              {freshness(device) === 'live'
                ? 'online'
                : (silenceWords(device) ?? `last reported ${relative(device.last_seen_at)}`)}
            </AppText>
          </View>

          {isSwitchable ? (
            <AppText
              variant="tiny"
              style={{ color: accent }}
              onPress={() => {
                const power = device.points.find((p) => p.capability === 'switch');
                if (power?.writable && device.online) {
                  setPoint.mutate({ code: power.code, value: power.value !== true });
                }
              }}
            >
              {device.points.find((p) => p.capability === 'switch')?.value === true ? 'Turn off' : 'Turn on'}
            </AppText>
          ) : null}
        </View>

        <View style={{ flexDirection: 'row', alignItems: 'flex-end', gap: 6 }}>
          <AppText
            style={{
              fontSize: 44,
              lineHeight: 48,
              fontWeight: '700',
              color: value === null || state !== 'live' ? color.muted : color.text,
            }}
          >
            {value === null ? '—' : value}
          </AppText>
          {point?.unit ? (
            <AppText variant="subhead" tone="muted" style={{ marginBottom: 8 }}>
              {point.unit}
            </AppText>
          ) : null}
        </View>

        {history?.buckets && history.buckets.length > 1 ? (
          <AreaChart
            buckets={
              value === null ? history.buckets : [...history.buckets, { ...history.buckets[history.buckets.length - 1]!, v: value }]
            }
            // A chart drawn in full-strength accent says "this is happening now". When the device has
            // gone quiet, the line keeps its shape and loses its authority.
            accent={state === 'live' ? accent : withAlpha(accent, 0.4)}
            height={78}
          />
        ) : (
          <AppText variant="tiny" tone="muted">
            No history for this reading yet.
          </AppText>
        )}

        <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}>
          <AppText variant="tiny" tone="muted">
            {rangeText}
          </AppText>
          <AppText variant="tiny" style={{ color: accent }}>
            {humanise(point?.capability)}
          </AppText>
        </View>
      </View>
    </Glass>
  );
}

/** The device the hero should show: online, numeric, and not a switch. */
export function pickHero(devices: Device[]): Device | null {
  const numeric = devices.filter((d) =>
    d.points.some((p) => typeof p.value === 'number' && !p.writable),
  );
  return (
    numeric.find((d) => d.online && d.category !== 'light') ??
    numeric.find((d) => d.online) ??
    numeric[0] ??
    null
  );
}

/** "4h ago", from an ISO stamp. A silent device's age is the most useful thing about it. */
export function relative(iso: string | null | undefined): string {
  if (!iso) return 'never';
  const ms = Date.now() - new Date(iso).getTime();
  if (Number.isNaN(ms) || ms < 0) return 'just now';
  const minutes = Math.round(ms / 60000);
  if (minutes < 2) return 'just now';
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

/** `color_temp` -> "Colour temperature": the wire name is not the label. */
function humanise(capability: string | undefined): string {
  if (!capability) return '';
  const words = capability.replace(/[._]+/g, ' ');
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export const heroSpacing = tokens.space.lg;
