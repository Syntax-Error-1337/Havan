import { View } from 'react-native';

import { useTheme, tokens } from '../../../app-shell/theme';
import { AppText, Glass, Icon, iconForCategory } from '../../../shared/ui';
import { accentFor, glowFor } from '../../../shared/config/tokens';
import { withAlpha } from '../../../shared/ui/Glass';
import { useSetPoint } from '../model/queries';
import { commandNotice } from '../model/notices';
import { primaryPoint, type Device } from '../model/types';
import { freshness, silenceWords } from '../model/silence';
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

  const status = (
    <View
      style={{
        flexDirection: 'row',
        alignItems: 'center',
        // Without this the pill stretches to the width of the column it sits in, which reads as a
        // status *bar* with a label at one end rather than a badge.
        alignSelf: 'flex-start',
        gap: 5,
        paddingHorizontal: 8,
        paddingVertical: 3,
        borderRadius: 999,
        backgroundColor: withAlpha(
          state === 'live' ? color.success : color.warning,
          // Silence is a question, not a fault: quieter than offline, so the eye still goes to the
          // device that is definitely not there.
          quiet ? 0.1 : 0.16,
        ),
      }}
    >
      <View
        style={{
          width: 6,
          height: 6,
          borderRadius: 3,
          backgroundColor: state === 'live' ? color.success : color.warning,
          opacity: quiet ? 0.7 : 1,
        }}
      />
      <AppText
        variant="tiny"
        style={{ color: state === 'live' ? color.success : color.warning, opacity: quiet ? 0.9 : 1 }}
      >
        {state === 'live' ? 'Online' : state === 'stale' ? 'Silent' : 'Offline'}
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
      <View style={{ flex: 1 }}>
        <AppText variant="title" weight="700" style={{ color: device.online ? accent : color.muted }} numberOfLines={1}>
          {readout}
        </AppText>
        {point?.capability ? (
          <AppText variant="tiny" tone="muted" numberOfLines={1}>
            {point.capability.replace(/[._]+/g, ' ')}
          </AppText>
        ) : null}
      </View>
    );

  return (
    <Glass
      onPress={onPress}
      accent={accent}
      glow={glow}
      style={{ flex: 1, minHeight: wide ? 104 : 132, opacity: device.online ? 1 : 0.8 }}
      testID={`device-tile-${device.serial}`}
      accessibilityLabel={`${device.name}, ${
        state === 'live' ? 'online' : state === 'stale' ? `silent, ${silent ?? 'not heard from'}` : 'offline'
      }`}
    >
      {/* One structure at both widths. Two layouts meant the status badge sat in a different place
          depending on how many neighbours a room had, which reads as a bug rather than a layout. */}
      <View style={{ gap: tokens.space.md, flex: 1 }}>
        <View style={{ flexDirection: 'row', alignItems: 'flex-start' }}>
          {icon}
          <View style={{ marginLeft: 'auto' }}>{status}</View>
        </View>

        <View style={{ gap: 2 }}>
          <AppText variant="subhead" weight="600" numberOfLines={1}>
            {device.name}
          </AppText>
          {/* What it is doing beats where it is: the room is the section heading already. */}
          <AppText variant="tiny" tone="muted" numberOfLines={1}>
            {stateLine(device, point)}
          </AppText>
        </View>

        <View
          style={{
            marginTop: 'auto',
            flexDirection: 'row',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: tokens.space.sm,
          }}
        >
          {control}
        </View>
      </View>

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
  // "Offline" alone does not say whether it dropped a minute ago or a week ago, and that difference is
  // what a person acts on. The pill already says which; this line says how long.
  const silent = silenceWords(device);
  if (!device.online || freshness(device) === 'stale') {
    return silent ?? 'Not heard from';
  }
  if (!point) return device.category;
  const value = point.value;
  if (typeof value === 'boolean') return value ? 'On' : 'Off';
  if (typeof value === 'number') {
    const label = point.slot?.includes('level') ? 'On' : 'Reading';
    return `${label} · ${value}${point.unit ?? ''}`;
  }
  return point.capability;
}
