import { Pressable, View } from 'react-native';

import { useTheme, tokens } from '../../../app-shell/theme';
import { AppText, Card, DeviceArtField, iconForCategory } from '../../../shared/ui';
import { accentFor } from '../../../shared/config/tokens';
import { reading, unitSuffix, valueWithUnit } from '../../../shared/ui/units';
import { useCommandOutcome, useSetPoint } from '../model/queries';
import { commandNotice, outcomeNotice } from '../model/notices';
import { primaryPoint, type Device } from '../model/types';
import { freshness, relative, silenceWords } from '../model/silence';
import { PillSwitch, controlFor } from './controls';

/**
 * One device as a tile.
 *
 * A grouped list row told you what a device was called; this tells you what it *is* before you read the name — the
 * glyph comes from the product's category, the field behind it comes from the same colour, and the control comes from
 * the primary point's own type. That is what makes a bulb, a lock and a camera look like one designed set instead of
 * three screens that happen to share a background.
 *
 * Two shapes: a grid tile (stacked, half width) and a wide one for a room that holds a single device.
 *
 * The reading is the largest thing on the card now. It used to sit at body size *under* the device's name, which put
 * the emphasis on the label and left the number — the only reason anyone opens the app — to be hunted for.
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

  // "Online" is what the platform believes; silence is what it has actually heard. A device that stopped reporting
  // an hour ago is not online in any sense the customer cares about.
  const state = freshness(device);
  const silent = silenceWords(device);
  const live = state === 'live';

  const point = primaryPoint(device);
  const setPoint = useSetPoint(device.id);
  // The command was accepted; the device still has to answer. This is the half of a write that used to be silent.
  const outcome = useCommandOutcome(device.id, setPoint.data?.command_id ?? null);
  const kind = point ? controlFor(point) : 'readout';
  const on = point?.value === true || (typeof point?.value === 'number' && point.value > 0);
  const numeric = typeof point?.value === 'number';

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
        : // The same rounding as the detail screen: `18.27 °C` is what the sensor sent, not what the room is, and
          // this string is now spoken aloud as well as drawn.
          valueWithUnit(reading(point.value as number), point.unit)
      : '—';

  // The numeral and its unit are two elements: `°C` belongs beside a 34-point number, not inside it, so it sits a
  // size smaller on the same baseline instead of shrinking the number to fit the suffix.
  const numberPart = numeric ? reading(point!.value as number) : readout;
  const unitPart = numeric ? unitSuffix(point!.unit) : '';

  const status = (
    <View style={{ flexDirection: 'row', alignItems: 'center', gap: 5 }}>
      <View
        style={{
          width: 6,
          height: 6,
          borderRadius: 3,
          backgroundColor: live ? color.success : state === 'stale' ? color.warning : color.muted,
        }}
      />
      <AppText
        variant="tiny"
        numberOfLines={1}
        style={{
          // Offline is dim, not invisible: at 11 pt it still has to clear 4.5:1 on the card, and it is the one line
          // that says whether the thing in your hand is working.
          color: state === 'stale' ? color.warning : color.muted,
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
    ) : null;

  const label = `${device.name}, ${readout}, ${
    live ? 'online' : state === 'stale' ? `silent, ${silent ?? 'not heard from'}` : 'offline'
  }`;

  const notice = setPoint.isError
    ? commandNotice(setPoint.error)
    : outcomeNotice(outcome.data?.status);

  const failure = notice ? (
    <AppText
      variant="tiny"
      numberOfLines={2}
      style={{ paddingHorizontal: tokens.space.md, paddingBottom: tokens.space.sm, color: color.danger }}
    >
      {notice}
    </AppText>
  ) : null;

  // One anatomy at two widths, on purpose.
  //
  // The first version gave a full-width card the old list-row structure and a half-width card a new art field, so the
  // grid held two tile families that only shared a radius — a reviewer called it exactly that. There is one rule now:
  // **the field holds the reading and the control; the name and the state sit under it.** A wide card is the same
  // card with a longer field, not a different component.
  const fieldHeight = wide ? 60 : 84;

  return (
    <Card
      // A switch inside a button is a button inside a button, which the DOM forbids and a screen reader cannot
      // describe. So the card is the surface, the tile body is the pressable, and the control is its sibling.
      containsControls
      accent={accent}
      padded={false}
      testID={`device-tile-${device.serial}`}
      style={{ flex: 1, minHeight: wide ? 108 : 150 }}
    >
      <Pressable
        onPress={onPress}
        accessibilityRole="button"
        accessibilityLabel={label}
        style={{ flex: 1 }}
      >
      {/* Inset by 6: a product sits inside its frame rather than bleeding to the edge of the card. */}
      <View style={{ padding: 6 }}>
        <DeviceArtField
          accent={accent}
          glyph={iconForCategory(device.category)}
          height={fieldHeight}
          dimmed={!live}
        >
          {/* Named where the number is. "Offline · 1d ago" under the card is a fact about the device; this is a fact
              about *this number*, and it is the one a person misreads: a bright 21.5 °C from yesterday looks live. */}
          {live ? null : (
            <AppText
              variant="tiny"
              numberOfLines={1}
              style={{
                position: 'absolute',
                top: 7,
                left: 10,
                letterSpacing: 0.6,
                textTransform: 'uppercase',
                color: color.muted,
              }}
            >
              Last known
            </AppText>
          )}
          <View
            style={{
              position: 'absolute',
              left: 10,
              // The reading sits on the field's centre line for a wide card and on its floor for a tall one.
              bottom: wide ? fieldHeight / 2 - 16 : 4,
              flexDirection: 'row',
              alignItems: 'baseline',
            }}
          >
            <AppText
              variant={wide ? 'title' : 'numeral'}
              weight="300"
              numberOfLines={1}
              style={{
                letterSpacing: -0.5,
                // The number keeps its full weight in every state: it is the reason the card exists, and an
                // unreadable "18.3" is a worse defect than a stale one. What marks it as history is the field's
                // dimming and the LAST KNOWN tag — not a grey numeral that fails the contrast floor it sits on.
                color: color.text,
              }}
            >
              {numberPart}
            </AppText>
            {unitPart ? (
              <AppText variant="caption" tone="muted">
                {unitPart}
              </AppText>
            ) : null}
          </View>
        </DeviceArtField>
      </View>

      <View style={{ paddingHorizontal: tokens.space.md, paddingBottom: tokens.space.md, gap: 2 }}>
        <AppText variant="subhead" weight="600" numberOfLines={1}>
          {device.name}
        </AppText>
        {status}
      </View>

      {failure}
      </Pressable>

      {/* Over the field, and outside the pressable region: one tab stop for the tile, one for the switch. */}
      {control ? (
        <View style={{ position: 'absolute', top: 10, right: 14 }}>{control}</View>
      ) : null}
    </Card>
  );
}

/** One line that says what the device is doing, in the words a person would use. */
function stateLine(device: Device, point: ReturnType<typeof primaryPoint>): string {
  // One line, and it says both which and how long: "Offline" alone cannot tell a person whether the bulb dropped a
  // minute ago or a week ago, and that difference is what they act on. The state word and the age used to live in
  // two separate places on the tile, which is two sentences for one fact.
  if (!device.online) return `Offline · ${relative(device.last_seen_at)}`;
  if (freshness(device) === 'stale') return silenceWords(device) ?? 'Not heard from';
  if (!point) return device.category;
  const value = point.value;
  if (typeof value === 'boolean') return value ? 'On' : 'Off';
  if (typeof value === 'number') {
    // The number is the tile's headline, so this line answers the only question left about it: how old is it?
    const at = point.updated_at ?? device.last_seen_at;
    return at ? `Updated ${relative(at)}` : 'Reading';
  }
  return point.capability;
}
