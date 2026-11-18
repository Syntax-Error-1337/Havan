import { Pressable, View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText, AreaChart, Card, Gauge, Icon, iconForCapability } from '../../shared/ui';
import { withAlpha } from '../../shared/ui/Glass';
import { reading, valueWithUnit } from '../../shared/ui/units';
import { accentFor } from '../../shared/config/tokens';
import { commandNotice } from '../../entities/device/model/notices';
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
 * A home screen with a grid of equal tiles makes you read every tile to find the thing you opened the app for. One
 * card carries what matters — with its last day drawn under it, or a ring you can turn — and the grid below is for
 * everything else.
 *
 * The device is chosen from data (`pickHero`), not from a setting nobody has filled in. When a home has no such
 * device the card simply does not render; inventing a figure would be worse than the empty space.
 */
export function HeroCard({ device, onOpen }: { device: Device; onOpen?: () => void }) {
  const { color, dark } = useTheme();
  const accent = accentFor(device.category, color.accent, dark);

  // Points arrive sorted by code, so "first numeric" is `battery.level` — a percentage nobody opened the app for.
  // Preference, not order: what a person checks first, then what they can read.
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
   * `10 PM` / `2 AM`: the hour a bucket starts, for the axis under the chart — with the day when it is not today. A
   * 24-hour window that began yesterday evening reads as "10 PM" at 10 PM, which is the same hour twice and the
   * reason a twelve-hour gap looked like a bug.
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

  // The chart's own scale, in words: a line with no numbers on it answers "did it move" and nothing else. Min and max
  // are the two facts a home owner wants from a day of readings, and the range covers what the chart draws — the live
  // reading is a point on it. Leaving the live value out put a 21.5 above a printed maximum of 20.7, which is the
  // kind of contradiction a customer reads faster than any other part of the card.
  const readings = (history?.buckets?.map((b) => b.v) ?? []).concat(value === null ? [] : [value]);

  /**
   * A dial is for a value you can *turn*; a chart is for a value you can only *watch*.
   *
   * This is a correction to the plan. It said "gauge when the point has a minimum and a maximum" — and the demo data
   * showed why that is the wrong test: a room sensor's temperature has a range of −10…50 and is read-only, so a ring
   * would say "18.3 is 47 % of a range nobody chose", while a lamp's brightness (1…100, writable) is exactly the
   * thing a person turns. So the ring appears for a **writable** ranged point and the day's line stays for a reading,
   * because for a reading the history *is* the story.
   */
  const dial =
    value !== null &&
    // A dial you cannot turn is a lie. Offline, the ring is replaced by the plain number: the steppers would be
    // disabled, and a disabled dial is a control that says "press me" and then refuses.
    device.online &&
    point?.writable === true &&
    typeof point.min === 'number' &&
    typeof point.max === 'number' &&
    point.max > point.min
      ? { min: point.min as number, max: point.max as number }
      : null;

  /**
   * The line is the recorded history, untouched. The live reading joins it only when it belongs to the last bucket;
   * otherwise it is drawn as a separate marker past a gutter, because a reading taken at 21:30 is not a reading from
   * 03:36 and drawing it as one is a straight line between two facts that never met.
   */
  const chartBuckets = history?.buckets ?? [];
  // The same function that explains the gap draws the marker: two thresholds would eventually disagree.
  const silentHours = gapHours(chartBuckets, history?.bucket_secs ?? 3600);
  const liveIsSeparate = value !== null && silentHours !== null;

  // Empty when there is nothing to say. It used to read "no readings yet", which sat directly under "No history for
  // this reading yet." — two sentences for one fact, in two different voices — next to a "steady" that described a
  // day with no data in it.
  const rangeText =
    readings.length > 1
      ? `${valueWithUnit(`${Math.min(...readings).toFixed(1)}–${Math.max(...readings).toFixed(1)}`, point?.unit)} recorded`
      : '';

  // What the day did, in one number: a range says how far it moved, the delta says which way.
  const delta =
    readings.length > 1 ? chartBuckets[chartBuckets.length - 1]!.v - chartBuckets[0]!.v : null;
  const direction = delta === null || Math.abs(delta) < 0.05 ? null : delta > 0 ? '↑' : '↓';

  const step = dial ? stepFor(dial.min, dial.max) : 0;
  const nudge = (sign: 1 | -1) => {
    if (!point || value === null || !dial) return;
    const next = Math.min(dial.max, Math.max(dial.min, value + sign * step));
    setPoint.mutate({ code: point.code, value: next });
  };

  return (
    <Card accent={accent} level={2} containsControls padded={false} testID="hero-card">
      {/* The card is a surface; the pressable part is the reading. A `<button>` cannot contain a button, and the
          steppers below are buttons — found by pressing one of them and reading the DOM. */}
      <Pressable
        onPress={onOpen}
        accessibilityRole="button"
        // The card's headline is the reading; a label that said only the name made the richest card on the screen the
        // emptiest one to a screen reader.
        accessibilityLabel={`${device.name}, ${
          value === null ? 'no reading' : valueWithUnit(reading(value), point?.unit)
        }, ${state === 'live' ? 'online' : 'last reading'} — open`}
      >
        <View style={{ gap: 14, padding: 18 }}>
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
              {/* The line describes the *reading*, not the connection. "online" next to a chart that stopped eleven
                  hours ago is true and reads as a contradiction: the device is reachable and the number is old. When
                  the series has a gap, the gap is the fact worth printing. */}
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
          </View>

          {dial ? (
            // The ring is the card's data moment: the value, where it sits in its range, and — where the unit is not a
            // percentage — the two ends of the scale. Everything a four-line block says with words, this says with a
            // shape.
            <View style={{ alignItems: 'center' }}>
              <Gauge
                value={value ?? dial.min}
                min={dial.min}
                max={dial.max}
                accent={state === 'live' ? accent : withAlpha(accent, 0.6)}
                size={146}
                caption={humanise(point?.capability)}
                showEnds={point?.unit !== '%'}
              >
                <View style={{ flexDirection: 'row', alignItems: 'baseline' }}>
                  <AppText variant="gauge" weight="200" style={{ letterSpacing: -1 }}>
                    {value === null ? '—' : reading(value)}
                  </AppText>
                  {point?.unit ? (
                    <AppText variant="caption" tone="muted">
                      {point.unit}
                    </AppText>
                  ) : null}
                </View>
              </Gauge>
            </View>
          ) : (
            <View style={{ flexDirection: 'row', alignItems: 'flex-end', gap: 6 }}>
              <AppText
                style={{
                  fontSize: 44,
                  lineHeight: 48,
                  fontWeight: '700',
                  // The number is dimmed once it is no longer the house's current state — and an old reading is old
                  // even when the device is reachable. A 44-point figure is the most authoritative thing on the
                  // screen, so it must not be the one that lies; but 55 %-opacity grey reads as "disabled", not "old".
                  // 74 % still clears 3:1 at this size and keeps the number legible.
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
          )}

          {/* One data moment per card: when the ring is showing, the day's line is not — two answers to the same
              question on one card is the fault this whole revamp is fixing. The "no history" sentence goes with the
              chart, not with the card: a dial has nothing to say about yesterday. */}
          {dial ? null : chartBuckets.length > 1 ? (
            <View style={{ gap: 4 }}>
              <AreaChart
                buckets={chartBuckets}
                // A chart drawn in full-strength accent says "this is happening now". When the device has gone quiet,
                // the line keeps its shape and loses its authority.
                accent={state === 'live' && !liveIsSeparate ? accent : withAlpha(accent, 0.58)}
                now={liveIsSeparate ? value : null}
                // The marker sits where the reading was taken, from the point's own clock.
                nowAt={point?.updated_at ? Date.parse(point.updated_at) : undefined}
                gapColor={withAlpha(color.muted, 0.55)}
                height={78}
              />
              {/* An axis, however small: a shape with no ends answers "did it move" and leaves "over what" to the
                  reader. Two labels are enough for a day. */}
              <View style={{ flexDirection: 'row', justifyContent: 'space-between' }}>
                <AppText variant="tiny" tone="muted">
                  {hourLabel(chartBuckets[0]?.t)}
                </AppText>
                {/* The right label says where the line actually ends. When the live reading is detached from it,
                    "now" would claim the line reaches the present — the label names the last bucket instead, and the
                    marker at the edge is the present. */}
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

          {/* Nothing to compare, nothing to say: a day with no readings is not steady, it is unrecorded. */}
          <View
            style={{
              flexDirection: 'row',
              justifyContent: 'space-between',
              alignItems: 'center',
              display: !dial && readings.length > 1 ? 'flex' : 'none',
            }}
          >
            <AppText variant="tiny" tone="muted">
              {rangeText}
            </AppText>
            {/* The gap is already named twice — in the subtitle and on the value's `last reading` tag — so the footer
                does the job it alone can do: which way the day went. When there is a gap the arrow would describe a
                window with a hole in it, so it is omitted rather than explained again. */}
            <AppText variant="tiny" style={{ color: state === 'live' ? accent : color.muted }}>
              {liveIsSeparate
                ? ''
                : direction === null
                  ? 'steady'
                  : `${direction} ${valueWithUnit(Math.abs(delta ?? 0).toFixed(1), point?.unit)}`}
            </AppText>
          </View>
        </View>
      </Pressable>

      {/* The command's own answer, where the command was given. The stepper writes to a device that may not reply —
          and a press that silently does nothing is the failure a customer reports as "the app is broken". */}
      {setPoint.isError ? (
        <AppText
          variant="tiny"
          tone="danger"
          style={{ paddingHorizontal: 18, paddingBottom: tokens.space.sm }}
        >
          {commandNotice(setPoint.error)}
        </AppText>
      ) : null}

      {/* A ring with no affordance is a picture of a dial. These are the affordance: two 44-point, keyboard-reachable
          steps — siblings of the pressable reading, never children of it. */}
      {dial ? (
        <View
          style={{
            flexDirection: 'row',
            alignItems: 'center',
            justifyContent: 'center',
            gap: tokens.space.xl,
            // Tight against the ring, with the card's own padding under it: the first version left a gap above the
            // steppers and pressed them into the bottom edge, which read as bottom-heavy for no reason.
            marginTop: -tokens.space.sm,
            paddingBottom: tokens.space.md,
          }}
        >
          <StepButton
            label={`Decrease ${humanise(point?.capability).toLowerCase()}`}
            sign="−"
            disabled={!device.online || setPoint.isPending || value === null || value <= dial.min}
            onPress={() => nudge(-1)}
          />
          <StepButton
            label={`Increase ${humanise(point?.capability).toLowerCase()}`}
            sign="+"
            disabled={!device.online || setPoint.isPending || value === null || value >= dial.max}
            onPress={() => nudge(1)}
          />
        </View>
      ) : null}
    </Card>
  );
}

/**
 * How far one press moves the dial.
 *
 * A percentage moves in fives, because that is how a person thinks about brightness. Anything else moves in twentieths
 * of its range, so a 2 000–6 500 K control takes a sensible step instead of 1 K per press.
 */
function stepFor(min: number, max: number): number {
  const span = max - min;
  return span <= 100 ? 5 : Math.max(1, Math.round(span / 20));
}

/** A 44-point stepper. */
function StepButton({
  sign,
  label,
  disabled,
  onPress,
}: {
  sign: string;
  label: string;
  disabled: boolean;
  onPress: () => void;
}) {
  const { color } = useTheme();
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityRole="button"
      accessibilityLabel={label}
      style={{
        width: tokens.touch.min,
        height: tokens.touch.min,
        borderRadius: tokens.touch.min / 2,
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: disabled ? color.track : withAlpha(color.text, 0.08),
        opacity: disabled ? 0.5 : 1,
      }}
    >
      {/* The glyph carries the state. At 60 % muted on a dark chip it read as disabled even when it was live. */}
      <AppText
        variant="title"
        weight="500"
        style={{ color: disabled ? color.muted : color.text, opacity: disabled ? 1 : 0.92 }}
      >
        {sign}
      </AppText>
    </Pressable>
  );
}

/**
 * The device the hero should show.
 *
 * The rule is "what a person opened the app to check", and it has four layers:
 *
 *   1. a live sensor — the reading that is happening now;
 *   2. a live device of any kind, because a lamp's dial is also worth the top of the screen;
 *   3. on a home where nothing is reporting, a **writable** device outranks a stale reading: yesterday's temperature
 *      is a fact you cannot act on, and the thing you *can* act on is the lamp;
 *   4. anything numeric at all, rather than an empty space.
 *
 * A switch-only device is never the hero: "On" is not a reading, and the grid already carries its toggle.
 */
export function pickHero(devices: Device[]): Device | null {
  const sensory = devices.filter((d) =>
    d.points.some((p) => typeof p.value === 'number' && !p.writable),
  );
  const actionable = devices.filter((d) =>
    d.points.some(
      (p) => typeof p.value === 'number' && p.writable && p.min !== null && p.max !== null,
    ),
  );

  return (
    sensory.find((d) => d.online && d.category !== 'light') ??
    sensory.find((d) => d.online) ??
    actionable.find((d) => d.online) ??
    actionable[0] ??
    sensory[0] ??
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
