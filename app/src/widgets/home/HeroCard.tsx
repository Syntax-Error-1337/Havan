import { View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText, AreaChart, Glass, Icon, iconForCapability } from '../../shared/ui';
import { withAlpha } from '../../shared/ui/Glass';
import { reading, valueWithUnit } from '../../shared/ui/units';
import { accentFor, glowFor } from '../../shared/config/tokens';
import {
  freshness,
  relative,
  gapHours,
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
export function HeroCard({ device, onOpen }: { device: Device; onOpen?: () => void }) {
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

  /**
   * `10 PM` / `2 AM`: the hour a bucket starts, for the axis under the chart — with the day when it is not
   * today. A 24-hour window that began yesterday evening reads as "10 PM" at 10 PM, which is the same hour
   * twice and the reason a twelve-hour gap looked like a bug.
   */
  const hourLabel = (iso: string | undefined): string => {
    if (!iso) return '';
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return '';
    const hour = d.toLocaleTimeString([], { hour: 'numeric' }).replace(':00', '');
    const day = d.toLocaleDateString([], { day: 'numeric', month: 'short' });
    const today = new Date().toLocaleDateString([], { day: 'numeric', month: 'short' });
    const yesterday = new Date(Date.now() - 86_400_000).toLocaleDateString([], {
      day: 'numeric',
      month: 'short',
    });
    if (day === today) return hour;
    // "Yesterday" in full: `Yest 7 AM` looked like a typo, and the axis has room for the word.
    return `${day === yesterday ? 'Yesterday' : day} ${hour}`;
  };

  // The chart's own scale, in words: a line with no numbers on it answers "did it move" and
  // nothing else. Min and max are the two facts a home owner actually wants from a day of readings.
  // The range covers what the chart draws, and the chart's axis now runs to *now* — the live reading is a
  // point on it. Leaving the live value out put a 21.5 above a printed maximum of 20.7, which is the kind of
  // contradiction a customer reads faster than any other part of the card.
  const readings = (history?.buckets?.map((b) => b.v) ?? []).concat(value === null ? [] : [value]);
  const isSwitchable = device.points.some((p) => p.capability === 'switch' && p.writable);

  /**
   * The line is the recorded history, untouched. The live reading joins it only when it belongs to the
   * last bucket; otherwise it is drawn as a separate marker past a gutter, because a reading taken at
   * 21:30 is not a reading from 03:36 and drawing it as one is a straight line between two facts that
   * never met — which is what made this chart look broken for two iterations.
   */
  const chartBuckets = history?.buckets ?? [];
  const lastBucket = chartBuckets[chartBuckets.length - 1];
  // The same function that explains the gap draws the marker: two thresholds would eventually disagree.
  const silentHours = gapHours(chartBuckets, history?.bucket_secs ?? 3600);
  const liveIsSeparate = value !== null && silentHours !== null;

  // The range, then the window it actually covers. "last 24 h" claimed a full day even when the readings
  // stopped eleven hours ago — the same overstatement the detached marker exists to avoid.
  const rangeText =
    readings.length > 1
      ? `${valueWithUnit(`${Math.min(...readings).toFixed(1)}–${Math.max(...readings).toFixed(1)}`, point?.unit)} recorded`
      : 'no readings yet';

  // What the day did, in one number: a range says how far it moved, the delta says which way.
  const delta =
    readings.length > 1 ? chartBuckets[chartBuckets.length - 1]!.v - chartBuckets[0]!.v : null;
  const direction = delta === null || Math.abs(delta) < 0.05 ? null : delta > 0 ? '↑' : '↓';



  return (
    <Glass
      accent={accent}
      onPress={onOpen}
      // The card's headline is the reading; a label that said only the name made the richest card on the
      // screen the emptiest one to a screen reader.
      accessibilityLabel={`${device.name}, ${
        value === null ? 'no reading' : valueWithUnit(reading(value), point?.unit)
      }, ${state === 'live' ? 'online' : 'last reading'} — open`}
      testID="hero-card"
      glow={state === 'live' ? glow : undefined}
      // Staleness is expressed per element — icon, chart, dot, the "last reading" tag — and *not* as an
      // opacity on the card. Dimming a subtree dims its text with it: measured in light mode, an 0.88 card
      // pulled an 11-point caption from 5.15:1 to **3.49:1**, below AA, and the text was the thing the
      // customer needed to read.
      style={{ padding: 18 }}
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
            <Icon
              name={iconForCapability(point?.capability)}
              size={17}
              color={state === 'live' ? accent : withAlpha(accent, 0.55)}
            />
          </View>
          <View style={{ flex: 1 }}>
            <AppText variant="caption" weight="600" numberOfLines={1}>
              {device.name}
            </AppText>
            {/* The line describes the *reading*, not the connection. "online" next to a chart that stopped
                eleven hours ago is true and reads as a contradiction: the device is reachable and the number
                is old. When the series has a gap, the gap is the fact worth printing. */}
            <AppText variant="tiny" tone="muted" numberOfLines={1} style={{ flexShrink: 1 }}>
              {device.room_name ?? 'No room'} ·{' '}
              {liveIsSeparate
                ? `${silentHours} h since the last reading`
                : freshness(device) === 'live'
                  ? 'online'
                  : (silenceWords(device) ?? `last reported ${relative(device.last_seen_at)}`)}
            </AppText>
          </View>
          {onOpen ? <Icon name="chevron-right" size={16} color={color.muted} /> : null}

          {isSwitchable ? (
            <AppText
              variant="tiny"
              style={{ color: color.accentInk }}
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
              // The number is dimmed once it is no longer the house's current state — and an old reading is
              // old even when the device is reachable. A 44-point figure is the most authoritative thing on
              // the screen, so it must not be the one that lies; but 55%-opacity grey reads as "disabled",
              // not "old". 72% white still clears 3:1 at this size and keeps the number legible.
              color:
                value === null || liveIsSeparate || state !== 'live'
                  ? withAlpha(color.text, 0.74)
                  : color.text,
            }}
          >
            {value === null ? '—' : reading(value)}
          </AppText>
          {point?.unit ? (
            <AppText variant="subhead" tone="muted" style={{ marginBottom: 8 }}>
              {point.unit}
            </AppText>
          ) : null}
          {liveIsSeparate ? (
            <AppText variant="tiny" tone="muted" style={{ marginBottom: 10 }}>
              last reading
            </AppText>
          ) : null}
        </View>

        {chartBuckets.length > 1 ? (
          <View style={{ gap: 4 }}>
            <AreaChart
              buckets={chartBuckets}
              // A chart drawn in full-strength accent says "this is happening now". When the device has
              // gone quiet, the line keeps its shape and loses its authority.
              accent={state === 'live' && !liveIsSeparate ? accent : withAlpha(accent, 0.58)}
              now={liveIsSeparate ? value : null}
              // The marker sits where the reading was taken, from the point's own clock.
              nowAt={point?.updated_at ? Date.parse(point.updated_at) : undefined}
              gapColor={withAlpha(color.muted, 0.55)}
              height={78}
            />
            {/* An axis, however small: a shape with no ends answers "did it move" and leaves "over
                what" to the reader. Two labels are enough for a day. */}
            <View style={{ flexDirection: 'row', justifyContent: 'space-between' }}>
              <AppText variant="tiny" tone="muted">
                {hourLabel(chartBuckets[0]?.t)}
              </AppText>
              {/* The right label says where the line actually ends. When the live reading is detached
                  from it, "now" would claim the line reaches the present — the label names the last
                  bucket instead, and the marker at the edge is the present. */}
              <AppText variant="tiny" tone="muted">
                now
              </AppText>
            </View>
          </View>
        ) : (
          <AppText variant="tiny" tone="muted">
            No history for this reading yet.
          </AppText>
        )}

        <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}>
          <AppText variant="tiny" tone="muted">
            {rangeText}
          </AppText>
          {/* The gap is already named twice — in the subtitle and on the value's `last reading` tag — so the
              footer does the job it alone can do: which way the day went. When there is a gap the arrow would
              describe a window with a hole in it, so it is omitted rather than explained again. */}
          <AppText variant="tiny" style={{ color: state === 'live' ? accent : color.muted }}>
            {liveIsSeparate
              ? ''
              : direction === null
                ? 'steady'
                : `${direction} ${valueWithUnit(Math.abs(delta ?? 0).toFixed(1), point?.unit)}`}
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

export { relative } from '../../entities/device/model/silence';

/** `color_temp` -> "Colour temperature": the wire name is not the label. */
function humanise(capability: string | undefined): string {
  if (!capability) return '';
  const words = capability.replace(/[._]+/g, ' ');
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export const heroSpacing = tokens.space.lg;
