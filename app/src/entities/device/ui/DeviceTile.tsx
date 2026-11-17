import { View } from 'react-native';

import { useTheme, tokens } from '../../../app-shell/theme';
import { AppText, Glass, Icon, iconForCategory } from '../../../shared/ui';
import { accentFor, glowFor } from '../../../shared/config/tokens';
import { withAlpha } from '../../../shared/ui/Glass';
import { useSetPoint } from '../model/queries';
import { commandNotice } from '../model/notices';
import { primaryPoint, type Device } from '../model/types';
import { freshness, relative, silenceWords } from '../model/silence';
import { PillSwitch, controlFor } from './controls';

/**
 * One device as a tile.
 *
 * A grouped list row told you what a device was called; this tells you what it is before you read the
 * name — the glyph comes from the product's category, the colour from the same category, and the
 * control from the primary point's own type. That is what makes a bulb, a lock and a camera look like
 * one designed set instead of three screens that happen to share a background.
 *
 * Two shapes: a grid tile (stacked, half width) and a wide one for a room that holds a single device.
 * The wide one puts the control beside the name, because a card with one line of text and a switch
 * far below it is a card with a hole in the middle.
 */
export function DeviceTile({
  device,
  onPress,
  wide = false,
}: {
  device: Device;
  onPress: () => void;
  /** A section with one device gives it the row: a half-width card alone reads as a missing tile. */
  wide?: boolean;
}) {
  const { color, dark } = useTheme();
  const accent = accentFor(device.category, color.accent, dark);
  const glow = glowFor(device.category, color.glow);

  // "Online" is what the platform believes; silence is what it has actually heard. A device that
  // stopped reporting an hour ago is not online in any sense the customer cares about.
  const state = freshness(device);
  const silent = silenceWords(device);
  const quiet = state === 'stale';

  const point = primaryPoint(device);
  const setPoint = useSetPoint(device.id);
  const kind = point ? controlFor(point) : 'readout';
  const on = point?.value === true || (typeof point?.value === 'number' && point.value > 0);

  const toggle = () => {
    if (!point?.writable) return;
    const next = point.kind === 'bool' ? !on : on ? 0 : 100;
    setPoint.mutate({ code: point.code, value: next });
  };

  const readout =
    point && point.value !== null && point.value !== undefined
      ? typeof point.value === 'boolean'
        ? point.value
          ? 'On'
          : 'Off'
        : `${point.value}${point.unit ? ` ${point.unit}` : ''}`
      : '—';

  const icon = (
    <View
      style={{
        width: wide ? 44 : 40,
        height: wide ? 44 : 40,
        borderRadius: wide ? 22 : 20,
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: withAlpha(accent, dark ? 0.22 : 0.14),
      }}
    >
      <Icon name={iconForCategory(device.category)} size={wide ? 22 : 20} color={accent} filled={on} />
    </View>
  );

  // One statement, not two. This was a pill reading "Offline" with a line underneath reading
  // "Silent for 1h" — the same fact twice in two vocabularies, on every tile in a house where
  // something is wrong. The dot carries the colour, the words carry the detail.
  const status = (
    <View style={{ flexDirection: 'row', alignItems: 'center', gap: 5 }}>
      <View
        style={{
          width: 6,
          height: 6,
          borderRadius: 3,
          backgroundColor: state === 'live' ? color.success : state === 'stale' ? color.warning : color.muted,
          opacity: quiet ? 0.75 : 1,
        }}
      />
      <AppText
        variant="tiny"
        numberOfLines={1}
        style={{
          // Offline is dim, not invisible: at 10-point it still has to clear 4.5:1, and muted-on-a-tinted
          // card was the one thing a reviewer consistently could not read.
          color: state === 'stale' ? color.warning : withAlpha(color.text, state === 'live' ? 0.62 : 0.72),
        }}
      >
        {stateLine(device, point)}
      </AppText>
    </View>
  );

  const control =
    kind === 'toggle' && point?.writable ? (
      <PillSwitch
        on={on}
        accent={accent}
        disabled={!device.online || setPoint.isPending}
        label={`${device.name} ${on ? 'on' : 'off'}`}
        onChange={toggle}
      />
    ) : (
      <AppText
        variant="title"
        weight="700"
        style={{ color: state === 'live' ? accent : color.muted }}
        numberOfLines={1}
      >
        {readout}
      </AppText>
    );

  return (
    <Glass
      onPress={onPress}
      // The halo and the border are the tile's "this is alive" signal, and they were still full-strength on
      // a sensor whose reading was twelve hours old: the words said quiet, the card glowed. A stale tile
      // keeps its category colour but loses the halo, and the whole card steps back a little.
      // (The accent is passed opaque on purpose: `Glass` derives its own alphas from it, and handing it a
      // pre-faded colour made it fall back to the raw string — a 45%-yellow card, brighter than a live one.)
      accent={accent}
      glow={state === 'live' ? glow : undefined}
      style={{ flex: 1, minHeight: wide ? 104 : 132, opacity: state === 'live' ? 1 : 0.88 }}
      testID={`device-tile-${device.serial}`}
      accessibilityLabel={`${device.name}, ${
        state === 'live' ? 'online' : state === 'stale' ? `silent, ${silent ?? 'not heard from'}` : 'offline'
      }`}
    >
      {/* Two shapes, chosen by the width the tile actually has. A half-width tile is about 140 points of
          content: icon, name and a switch on one line leaves forty points for the name, and "Floor Lamp"
          becomes "F…". So the grid stacks (icon and control on top, name and state under it) and the
          single-device row, which has the whole width, puts them side by side. */}
      {wide ? (
        <View style={{ flex: 1, gap: tokens.space.sm, justifyContent: 'space-between' }}>
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: tokens.space.sm }}>
            {icon}
            <View style={{ flex: 1, gap: 1 }}>
              <AppText variant="subhead" weight="600" numberOfLines={1}>
                {device.name}
              </AppText>
              {status}
            </View>
            <View>{control}</View>
          </View>
        </View>
      ) : (
        <View style={{ flex: 1, gap: tokens.space.xs }}>
          <View style={{ flexDirection: 'row', alignItems: 'flex-start', gap: 6 }}>
            {icon}
            {kind === 'toggle' && point?.writable ? (
              <View style={{ marginLeft: 'auto', marginTop: 2 }}>{control}</View>
            ) : null}
          </View>
          <View style={{ gap: 1, marginTop: 'auto' }}>
            <AppText variant="subhead" weight="600" numberOfLines={1}>
              {device.name}
            </AppText>
            {status}
          </View>
          {kind === 'toggle' && point?.writable ? null : <View>{control}</View>}
        </View>
      )}

      {setPoint.isError ? (
        <AppText variant="tiny" numberOfLines={2} style={{ marginTop: 6, color: color.danger }}>
          {commandNotice(setPoint.error)}
        </AppText>
      ) : null}
    </Glass>
  );
}

/** One line that says what the device is doing, in the words a person would use. */
function stateLine(device: Device, point: ReturnType<typeof primaryPoint>): string {
  // One line, and it says both which and how long: "Offline" alone cannot tell a person whether the bulb
  // dropped a minute ago or a week ago, and that difference is what they act on. The state word and the age
  // used to live in two separate places on the tile, which is two sentences for one fact.
  if (!device.online) return `Offline · ${relative(device.last_seen_at)}`;
  if (freshness(device) === 'stale') return silenceWords(device) ?? 'Not heard from';
  if (!point) return device.category;
  const value = point.value;
  if (typeof value === 'boolean') return value ? 'On' : 'Off';
  if (typeof value === 'number') {
    // The number is the tile's headline, so this line answers the only question left about it: how old
    // is it? "Reading · 21.5 °C" above a large 21.5 °C was the same fact told twice.
    const at = point.updated_at ?? device.last_seen_at;
    return at ? `Updated ${relative(at)}` : 'Reading';
  }
  return point.capability;
}
