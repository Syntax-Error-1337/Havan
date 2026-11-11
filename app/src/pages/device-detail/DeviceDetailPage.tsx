import { useState } from 'react';
import { ActivityIndicator, Pressable, Switch } from 'react-native';
import { useLocalSearchParams } from 'expo-router';
import { Box, Screen, AppText, Sparkline } from '../../shared/ui';
import {
  RANGES,
  describeUpdate,
  useDevice,
  useDeviceUpdate,
  useHistory,
  useSetPoint,
  useSetRoom,
  type Range,
} from '../../entities/device';
import type { DevicePoint } from '../../entities/device';
import { useRooms } from '../../entities/home';
import { useTheme, tokens } from '../../app-shell/theme';

export function DeviceDetailPage() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { color } = useTheme();
  const { data: device, isPending, isError } = useDevice(id);
  const { data: update } = useDeviceUpdate(id);
  const firmware = describeUpdate(update);
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
          {/* Firmware is the one thing about a device a customer cannot fix by
              themselves, so a failure says why rather than only that it failed. */}
          {firmware ? (
            <AppText variant="caption" tone={firmware.bad ? 'danger' : 'muted'}>
              {firmware.text}
            </AppText>
          ) : null}
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

        <HistoryChart deviceId={device.id} points={device.points} />

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
function HistoryChart({ deviceId, points }: { deviceId: string; points: DevicePoint[] }) {
  // Chart the first numeric point: a switch's history is a square wave and is
  // the least interesting thing on the screen. `isBoolean` must describe *that*
  // point — deriving it from the device turned a 40–80 brightness chart into a
  // clamped 0–1 one.
  // Only points the platform actually stores. A chart for a point with
  // `history: off` is a promise that can never be kept — the screen would say
  // "no readings yet" forever, which reads as a bug rather than a policy.
  // An older server that does not send `history` keeps the old behaviour: show
  // the chart. Only an explicit `off` hides it.
  const stored = points.filter((p) => p.history !== 'off');
  const charted = stored.find((p) => typeof p.value === 'number') ?? stored[0];
  const code = charted?.code;
  const isBoolean = typeof charted?.value === 'boolean';
  // The range the chart is showing. Four options, because a phone chart with a
  // date picker is a spreadsheet with a smaller screen.
  const [range, setRange] = useState<Range>('1d');
  const { data, isPending, isError } = useHistory(deviceId, code, range);

  if (!code) {
    return (
      <Box gap="xs">
        <AppText variant="caption" tone="muted">
          This device does not keep history.
        </AppText>
      </Box>
    );
  }

  const samples = (data?.buckets ?? []).reduce((sum, b) => sum + b.n, 0);

  return (
    <Box gap="xs">
      <Box row style={{ justifyContent: 'space-between', alignItems: 'center' }}>
        <AppText variant="caption" tone="muted">
          {code}
        </AppText>
        <Box row gap="xs">
          {RANGES.map((option) => (
            <Pressable
              key={option}
              onPress={() => setRange(option)}
              accessibilityRole="button"
              accessibilityState={{ selected: range === option }}
            >
              <Box px="sm" py="xs" radius="pill" bg={range === option ? 'accent' : 'surface'}>
                <AppText variant="caption" tone={range === option ? 'text' : 'muted'}>
                  {option}
                </AppText>
              </Box>
            </Pressable>
          ))}
        </Box>
      </Box>

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
          {/* One string, not three children: a sentence assembled from nodes is
              also a sentence a test cannot match and a screen reader may split. */}
          <AppText variant="caption" tone="muted">
            {`${samples} reading${samples === 1 ? '' : 's'} · ${data?.buckets.length ?? 0} bars`}
          </AppText>
        </>
      )}
    </Box>
  );
}
