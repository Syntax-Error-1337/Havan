import { ActivityIndicator, Pressable, Switch } from 'react-native';
import { useLocalSearchParams } from 'expo-router';
import { Box, Screen, AppText, Sparkline } from '../../shared/ui';
import { useDevice, useHistory, useSetPoint, useSetRoom } from '../../entities/device';
import type { DevicePoint } from '../../entities/device';
import { useRooms } from '../../entities/home';
import { useTheme, tokens } from '../../app-shell/theme';

export function DeviceDetailPage() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { color } = useTheme();
  const { data: device, isPending, isError } = useDevice(id);
  const setPoint = useSetPoint(id ?? '');
  const setRoom = useSetRoom();
  const { data: rooms } = useRooms(device?.home_id ?? undefined);

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
          <AppText tone="muted">It may have been removed, or you may be offline.</AppText>
        </Box>
      </Screen>
    );
  }

  return (
    <Screen>
      <Box p="lg" gap="lg">
        <Box gap="xs">
          <AppText variant="display" weight="700">
            {device.name}
          </AppText>
          <AppText variant="caption" tone="muted">
            {device.online ? 'Online' : 'Offline'} · {device.product_key}
          </AppText>
        </Box>

        <Box gap="sm">
          <AppText variant="caption" tone="muted">
            Room
          </AppText>
          <Box row gap="sm" style={{ flexWrap: 'wrap' }}>
            {[{ id: null as string | null, name: 'None' }, ...(rooms ?? [])].map((room) => {
              const active = (device.room_id ?? null) === room.id;
              return (
                <Pressable
                  key={room.id ?? 'none'}
                  disabled={setRoom.isPending}
                  onPress={() => setRoom.mutate({ deviceId: device.id, roomId: room.id })}
                  accessibilityRole="button"
                  accessibilityState={{ selected: active }}
                >
                  <Box px="md" py="sm" radius="pill" bg={active ? 'accent' : 'surface'} border>
                    <AppText variant="caption" tone={active ? 'text' : 'muted'}>
                      {room.name}
                    </AppText>
                  </Box>
                </Pressable>
              );
            })}
          </Box>
          {setRoom.error ? (
            <AppText variant="caption" tone="danger">
              {setRoom.error.message}
            </AppText>
          ) : null}
        </Box>

        <HistoryChart
          deviceId={device.id}
          code={device.points.find((p) => typeof p.value === 'number')?.code ?? device.points[0]?.code}
          isBoolean={typeof device.points.find((p) => typeof p.value === 'boolean')?.value === 'boolean'}
        />

        {device.points.map((point) => (
          <Box
            key={point.code}
            row
            style={{ justifyContent: 'space-between', alignItems: 'center', minHeight: 56 }}
          >
            <Box gap="xs" style={{ flexShrink: 1 }}>
              <AppText weight="500">{point.code}</AppText>
              <AppText variant="caption" tone="muted">
                {point.capability}
                {point.unit ? ` · ${point.unit}` : ''}
              </AppText>
            </Box>

            <Control
              point={point}
              disabled={!device.online || setPoint.isPending}
              onChange={(value) => setPoint.mutate({ code: point.code, value })}
            />
          </Box>
        ))}

        {setPoint.error ? (
          <AppText tone="danger" variant="caption">
            {setPoint.error.message}
          </AppText>
        ) : null}
      </Box>
    </Screen>
  );
}

function Control({
  point,
  disabled,
  onChange,
}: {
  point: DevicePoint;
  disabled: boolean;
  onChange: (value: boolean | number) => void;
}) {
  const { color } = useTheme();

  if (!point.writable) {
    return <AppText weight="600">{formatValue(point.value)}</AppText>;
  }

  if (typeof point.value === 'boolean') {
    return (
      <Switch
        value={point.value}
        onValueChange={onChange}
        disabled={disabled}
        accessibilityLabel={point.code}
        trackColor={{ true: color.accent, false: color.border }}
      />
    );
  }

  if (typeof point.value === 'number') {
    const step = point.kind === 'int' ? 1 : 0.5;
    const min = point.min ?? 0;
    const max = point.max ?? 100;
    const bump = (delta: number) => {
      const next = Math.min(max, Math.max(min, (point.value as number) + delta));
      onChange(Math.round(next * 10) / 10);
    };

    return (
      <Box row gap="sm" style={{ alignItems: 'center' }}>
        <Stepper label="−" onPress={() => bump(-step)} disabled={disabled} />
        <AppText weight="600" style={{ minWidth: 56, textAlign: 'center' }}>
          {formatValue(point.value)}
          {point.unit ?? ''}
        </AppText>
        <Stepper label="+" onPress={() => bump(step)} disabled={disabled} />
      </Box>
    );
  }

  return <AppText weight="600">{formatValue(point.value)}</AppText>;
}

function Stepper({
  label,
  onPress,
  disabled,
}: {
  label: string;
  onPress: () => void;
  disabled: boolean;
}) {
  const { color } = useTheme();
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityRole="button"
      accessibilityLabel={label === '+' ? 'increase' : 'decrease'}
      style={{
        width: tokens.touch.min,
        height: tokens.touch.min,
        borderRadius: tokens.radius.md,
        borderWidth: 1,
        borderColor: color.border,
        alignItems: 'center',
        justifyContent: 'center',
        opacity: disabled ? 0.4 : 1,
      }}
    >
      <AppText weight="600">{label}</AppText>
    </Pressable>
  );
}

function formatValue(value: boolean | number | string | null): string {
  if (value === null) return '—';
  if (typeof value === 'boolean') return value ? 'On' : 'Off';
  if (typeof value === 'number') return Number.isInteger(value) ? String(value) : value.toFixed(1);
  return String(value);
}

/**
 * A day of one datapoint, or an honest sentence about why there is no chart.
 */
function HistoryChart({
  deviceId,
  code,
  isBoolean,
}: {
  deviceId: string;
  code: string | undefined;
  isBoolean: boolean;
}) {
  const { data, isPending, isError } = useHistory(deviceId, code, '1d');

  if (!code) return null;

  const samples = (data?.buckets ?? []).reduce((sum, b) => sum + b.n, 0);

  return (
    <Box gap="xs">
      <AppText variant="caption" tone="muted">
        {code} · last 24h
      </AppText>

      {isPending ? (
        <AppText variant="caption" tone="muted">
          Loading history…
        </AppText>
      ) : isError ? (
        <AppText variant="caption" tone="muted">
          History is unavailable right now.
        </AppText>
      ) : samples === 0 ? (
        <AppText variant="caption" tone="muted">
          No readings yet. History starts after the device reports for a while.
        </AppText>
      ) : (
        <>
          <Sparkline buckets={data?.buckets ?? []} isBoolean={isBoolean} />
          <AppText variant="caption" tone="muted">
            {samples} reading{samples === 1 ? '' : 's'} · {data?.buckets.length} bars
          </AppText>
        </>
      )}
    </Box>
  );
}
