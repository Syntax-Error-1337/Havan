import { Pressable } from 'react-native';
import { Box, AppText } from '../../../shared/ui';
import { useTheme, tokens } from '../../../app-shell/theme';
import { useSetPoint } from '../model/queries';
import { primaryPoint, type Device } from '../model/types';

/**
 * The dashboard tile. It renders the device's *primary* point only; the full
 * capability set lives on the detail screen. A tile that tries to draw
 * everything is a tile nobody can scan.
 */
export function DeviceCard({ device, onPress }: { device: Device; onPress: () => void }) {
  const { color } = useTheme();
  const point = primaryPoint(device);
  const setPoint = useSetPoint(device.id);

  const isOn =
    point?.value === true || (typeof point?.value === 'number' && point.value > 0);

  const toggle = () => {
    if (!point?.writable) return;
    const next = typeof point.value === 'boolean' ? !point.value : isOn ? 0 : 100;
    setPoint.mutate({ code: point.code, value: next });
  };

  return (
    <Pressable
      onPress={onPress}
      onLongPress={toggle}
      accessibilityRole="button"
      accessibilityLabel={`${device.name}, ${isOn ? 'on' : 'off'}${
        device.online ? '' : ', offline'
      }`}
      style={({ pressed }) => ({
        flex: 1,
        minHeight: 96,
        borderRadius: tokens.radius.lg,
        backgroundColor: color.surface,
        borderWidth: 1,
        borderColor: isOn ? color.accent : color.border,
        padding: tokens.space.lg,
        justifyContent: 'space-between',
        opacity: pressed ? 0.85 : 1,
      })}
    >
      <Box row style={{ justifyContent: 'space-between', alignItems: 'center' }}>
        <AppText weight="600" numberOfLines={1} style={{ flexShrink: 1 }}>
          {device.name}
        </AppText>
        <Box
          style={{
            width: 8,
            height: 8,
            borderRadius: 4,
            backgroundColor: device.online ? color.success : color.muted,
          }}
        />
      </Box>

      <AppText variant="caption" tone="muted" numberOfLines={1}>
        {point ? `${point.code}${point.unit ? ` · ${point.unit}` : ''}` : device.category}
      </AppText>
    </Pressable>
  );
}
