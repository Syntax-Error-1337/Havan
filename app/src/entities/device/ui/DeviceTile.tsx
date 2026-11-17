import { View } from 'react-native';

import { useTheme, tokens } from '../../../app-shell/theme';
import { AppText, Glass, Icon, iconForCategory } from '../../../shared/ui';
import { accentFor, glowFor } from '../../../shared/config/tokens';
import { withAlpha } from '../../../shared/ui/Glass';
import { useSetPoint } from '../model/queries';
import { commandNotice } from '../model/notices';
import { primaryPoint, type Device } from '../model/types';
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
  const accent = accentFor(device.category, color.accent);
  const glow = glowFor(device.category, color.glow);

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
        backgroundColor: withAlpha(device.online ? color.success : color.warning, 0.16),
      }}
    >
      <View
        style={{
          width: 6,
          height: 6,
          borderRadius: 3,
          backgroundColor: device.online ? color.success : color.warning,
        }}
      />
      <AppText variant="tiny" style={{ color: device.online ? color.success : color.warning }}>
        {device.online ? 'Online' : 'Offline'}
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
      <AppText variant="title" weight="700" style={{ color: accent }} numberOfLines={1}>
        {readout}
      </AppText>
    );

  return (
    <Glass
      onPress={onPress}
      accent={accent}
      glow={glow}
      style={{ flex: 1, minHeight: wide ? 104 : 132, opacity: device.online ? 1 : 0.8 }}
      testID={`device-tile-${device.serial}`}
      accessibilityLabel={`${device.name}, ${device.online ? 'online' : 'offline'}`}
    >
      {wide ? (
        <View style={{ flexDirection: 'row', alignItems: 'center', gap: tokens.space.md }}>
          {icon}
          <View style={{ flex: 1, gap: 2 }}>
            <AppText variant="subhead" weight="600" numberOfLines={1}>
              {device.name}
            </AppText>
            {status}
          </View>
          {control}
        </View>
      ) : (
        <View style={{ gap: tokens.space.md, flex: 1 }}>
          <View style={{ flexDirection: 'row', alignItems: 'flex-start' }}>
            {icon}
            <View style={{ marginLeft: 'auto' }}>{status}</View>
          </View>

          <View style={{ gap: 2 }}>
            <AppText variant="subhead" weight="600" numberOfLines={1}>
              {device.name}
            </AppText>
            {device.room_name ? (
              <AppText variant="tiny" tone="muted" numberOfLines={1}>
                {device.room_name}
              </AppText>
            ) : null}
          </View>

          <View style={{ marginTop: 'auto', flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' }}>
            {control}
          </View>
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
