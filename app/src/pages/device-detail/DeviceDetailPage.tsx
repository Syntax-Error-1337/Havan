import { ActivityIndicator } from 'react-native';
import { useLocalSearchParams } from 'expo-router';
import { Box, Screen, AppText } from '../../shared/ui';
import { useDevice } from '../../entities/device';
import { useTheme } from '../../app/theme';

export function DeviceDetailPage() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { color } = useTheme();
  const { data: device, isPending, isError } = useDevice(id);

  if (isPending) {
    return (
      <Screen>
        <Box center flex={1}>
          <ActivityIndicator color={color.accent} />
        </Box>
      </Screen>
    );
  }

  if (isError || !device) {
    return (
      <Screen>
        <Box p="xl" gap="sm">
          <AppText variant="title" weight="600">
            Device unavailable
          </AppText>
          <AppText tone="muted">
            It may have been removed, or you may be offline.
          </AppText>
        </Box>
      </Screen>
    );
  }

  return (
    <Screen>
      <Box p="lg" gap="lg">
        <AppText variant="display" weight="700">
          {device.name}
        </AppText>

        {device.points.map((point) => (
          <Box
            key={point.code}
            row
            style={{
              justifyContent: 'space-between',
              alignItems: 'center',
              minHeight: 56,
            }}
          >
            <Box gap="xs">
              <AppText weight="500">{point.code}</AppText>
              <AppText variant="caption" tone="muted">
                {point.capability}
                {point.unit ? ` · ${point.unit}` : ''}
              </AppText>
            </Box>
            <AppText weight="600">{formatValue(point.value)}</AppText>
          </Box>
        ))}
      </Box>
    </Screen>
  );
}

function formatValue(value: boolean | number | string | null): string {
  if (value === null) return '—';
  if (typeof value === 'boolean') return value ? 'On' : 'Off';
  return String(value);
}
