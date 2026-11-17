import { useState } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, Switch, TextInput } from 'react-native';
import { useLocalSearchParams, useRouter } from 'expo-router';
import {
  Box,
  Screen,
  AppText,
  Glass,
  Icon,
  Sparkline,
  iconForCategory,
  useConfirm,
} from '../../shared/ui';
import { PointControl } from '../../entities/device';
import { accentFor, glowFor } from '../../shared/config/tokens';
import {
  RANGES,
  RANGE_WORDS,
  chartPoint,
  titleFor,
  describeUpdate,
  expectedReportWords,
  freshness,
  quietAfterWords,
  relative,
  silenceWords,
  useDevice,
  useDeviceUpdate,
  useHistory,
  useRemoveDevice,
  useRenameDevice,
  useSetPoint,
  useSetRoom,
  type Range,
} from '../../entities/device';
import type { DevicePoint } from '../../entities/device';
import { canManage, useHomes, useRooms } from '../../entities/home';
import { useTheme, tokens } from '../../app-shell/theme';
import { useInlineEdit } from '../../shared/lib/useInlineEdit';

export function DeviceDetailPage() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { color, dark } = useTheme();
  const { data: device, isPending, isError } = useDevice(id);
  const accent = accentFor(device?.category, color.accent, dark);
  const glow = glowFor(device?.category, color.glow);
  const { data: update } = useDeviceUpdate(id);
  const firmware = describeUpdate(update);
  // Only shown when the product declared an expectation; a device that never promised anything has
  // nothing to be late for, and inventing a schedule for it would be a lie with a number on it.
  const expectedReport = device ? expectedReportWords(device) : null;
  // The age of the last thing it said, which is the number a person actually wants.
  const lastReported = device?.last_seen_at ? relative(device.last_seen_at) : null;
  const setPoint = useSetPoint(id ?? '');
  const setRoom = useSetRoom();
  const rename = useRenameDevice();
  const remove = useRemoveDevice();
  const router = useRouter();
  const { ask, sheet } = useConfirm();

  /**
   * Asking is the point: this is the one action on the screen that cannot be undone by pressing
   * something else. The wording says what happens rather than "are you sure" — "sure" is a question
   * about the person, and this sentence should be about the device.
   */
  const confirmRemove = () => {
    if (!device?.home_id) return;
    const homeId = device.home_id;
    ask({
      title: `Remove ${device.name}?`,
      message:
        'It leaves this home and stops appearing in the app. To use it again, pair it again. Its licence stays with the platform and its history is kept.',
      confirmLabel: 'Remove',
      cancelLabel: 'Keep it',
      destructive: true,
      onConfirm: () =>
        remove.mutate(
          { deviceId: device.id, homeId },
          // Back to the home it is no longer in: leaving the customer on a page about a device they just
          // removed is how an app feels broken.
          { onSuccess: () => router.replace('/') },
        ),
    });
  };

  // Hooks above the early returns, as always: `device` is undefined until the query resolves.
  const nameEdit = useInlineEdit(device?.name ?? '', (name) => {
    if (id) rename.mutate({ deviceId: id, name });
  });
  const { data: rooms } = useRooms(device?.home_id ?? undefined);
  const { data: homes } = useHomes(false);
  // Renaming is an admin action on the server. A control that exists only to be refused is worse than
  // no control: the member taps their device's name, types, presses Save and is told no.
  const canManageHome = canManage(homes?.find((h) => h.id === device?.home_id)?.role ?? 'member');

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
      {sheet}
      <Box p="lg" gap="lg">
        {/* The hero is the device's own category: a bulb glows violet, a lock pink, a camera blue —
            chosen from data, so a product we have never seen still looks deliberate. */}
        <Glass accent={accent} glow={glow} style={{ padding: 20 }}>
        <Box gap="xs" style={{ flexDirection: 'row', alignItems: 'center', gap: 14 }}>
          <Box
            style={{
              width: 56,
              height: 56,
              borderRadius: 28,
              alignItems: 'center',
              justifyContent: 'center',
              backgroundColor: accent + '33',
            }}
          >
            <Icon name={iconForCategory(device.category)} size={28} color={accent} />
          </Box>
          <Box gap="xs" style={{ flex: 1 }}>
          {nameEdit.editing ? (
            // In place, not on another screen: renaming a device is the one edit a customer makes
            // while looking at it, and a form that pushes over the thing being renamed hides the
            // context that made them rename it.
            <Box row gap="sm" style={{ alignItems: 'center' }}>
              <TextInput
                value={nameEdit.draft}
                onChangeText={nameEdit.setDraft}
                autoFocus
                placeholder="Device name"
                placeholderTextColor={color.muted}
                onSubmitEditing={nameEdit.save}
                accessibilityLabel="Device name"
                style={{
                  flex: 1,
                  color: color.text,
                  fontSize: 22,
                  fontWeight: '700',
                  borderBottomWidth: 1,
                  borderBottomColor: color.accent,
                  paddingVertical: 2,
                }}
              />
              <Pressable
                onPress={nameEdit.save}
                testID="rename-save"
                accessibilityRole="button"
                hitSlop={10}
              >
                <AppText tone="accent">Save</AppText>
              </Pressable>
              <Pressable
                onPress={nameEdit.cancel}
                testID="rename-cancel"
                accessibilityRole="button"
                hitSlop={10}
              >
                <AppText tone="muted">Cancel</AppText>
              </Pressable>
            </Box>
          ) : canManageHome ? (
            <Pressable
              onPress={nameEdit.open}
              testID="rename-device"
              accessibilityRole="button"
              accessibilityLabel={`Rename ${device.name}`}
              accessibilityHint="Opens an editor for the device's name"
            >
              <AppText variant="title" weight="700">
                {device.name}
              </AppText>
            </Pressable>
          ) : (
            <AppText variant="title" weight="700">
              {device.name}
            </AppText>
          )}
          {rename.isError ? (
            <AppText variant="tiny" tone="danger">
              {rename.error instanceof Error ? rename.error.message : 'That name was not saved.'}
            </AppText>
          ) : null}
          <AppText variant="caption" tone="muted">
            {freshness(device) === 'live'
              ? 'Online'
              : freshness(device) === 'stale'
                ? (silenceWords(device) ?? 'Silent')
                : 'Offline'}
            {device.room_name ? ` · ${device.room_name}` : ''}
          </AppText>
          {/* What the product promised, so "Silent for 4h" reads as a fact rather than a mystery: an
              hourly sensor is late at four hours, a plug that reports every ten seconds is late at one
              minute, and the customer can only judge the number if they know the promise behind it. */}
          {/* What it promises, and when it last kept the promise. "Quiet after 3h without a report" told a
              customer the threshold the platform uses — a policy, not a fact about their device. */}
          {expectedReport ? (
            <AppText variant="tiny" tone="muted">
              {expectedReport}
              {lastReported ? ` · last reported ${lastReported}` : ''}
            </AppText>
          ) : lastReported ? (
            <AppText variant="tiny" tone="muted">
              Last reported {lastReported}
            </AppText>
          ) : null}
          </Box>
        </Box>
          {/* Firmware is the one thing about a device a customer cannot fix by
              themselves, so a failure says why rather than only that it failed. */}
          {firmware ? (
            <AppText variant="caption" tone={firmware.bad ? 'danger' : 'muted'} style={{ marginTop: 10 }}>
              {firmware.text}
            </AppText>
          ) : null}
        </Glass>

        {/* Values below are the last ones this device reported. Saying so is the difference between
            "this is broken" and "this is what the house looked like when it went quiet". */}
        {freshness(device) !== 'live' ? (
          <Glass accent={color.warning}>
            <Box row gap="sm" style={{ alignItems: 'center' }}>
              <Icon name="alert" size={16} color={color.warning} />
              <AppText variant="caption" tone="muted" style={{ flex: 1 }}>
                {freshness(device) === 'stale'
                  ? // Online, and quiet. The platform still has a connection to it; what it does not
                    // have is a recent word from it, and the difference matters to someone deciding
                    // whether to walk over and look.
                    `Connected, but ${(silenceWords(device) ?? 'silent').toLowerCase()} — these are the last values it sent.`
                  : 'Offline — showing the last values this device reported.'}
              </AppText>
            </Box>
          </Glass>
        ) : null}

        <Box gap="xs">
          <AppText variant="caption" tone="muted">Room</AppText>
          <Box row gap="sm" style={{ flexWrap: 'wrap' }}>
            {[{ id: null as string | null, name: 'No room' }, ...(rooms ?? [])].map((room) => {
              const active = (device.room_id ?? null) === room.id;
              return (
                <Pressable
                  key={room.id ?? 'none'}
                  disabled={setRoom.isPending}
                  onPress={() => setRoom.mutate({ deviceId: device.id, roomId: room.id })}
                  accessibilityRole="button"
                  accessibilityState={{ selected: active }}
                >
                  <Box
                    px="md"
                    py="sm"
                    radius="pill"
                    style={{
                      backgroundColor: active ? color.accent : color.glass,
                      borderWidth: StyleSheet.hairlineWidth,
                      borderColor: active ? color.accent : color.border,
                    }}
                  >
                    <AppText variant="caption" style={{ color: active ? color.onAccent : color.muted }}>
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
          points={device.points}
          asOf={freshness(device) === 'live' ? null : relative(device.last_seen_at)}
        />

        {/*
          Controls come from the device's own data: a bool is a switch, a bounded number is a level,
          a temperature is a stepper, and a sensor is a reading. That is what makes an unfamiliar
          product usable on the day its vendor ships it.
        */}
        {/* One place decides that a silent device's controls are inert, rather than each control
            remembering to dim itself. */}
        <Box gap="md" style={{ opacity: device.online ? 1 : 0.6 }}>
          {orderedPoints(device.points).map((point) => (
            <Glass key={point.code} accent={point.writable ? accent : undefined}>
              <PointControl device={device} point={point} accent={accent} />
            </Glass>
          ))}
        </Box>

        {setPoint.error ? (
          <AppText tone="danger" variant="caption">
            {setPoint.error.message}
          </AppText>
        ) : null}

        {/* Last, quiet, and destructive: the bottom of the screen is where "I am done with this" lives,
            not next to the switch someone presses ten times a day. */}
        {canManageHome ? (
          <Box gap="xs" style={{ paddingTop: tokens.space.md }}>
            <Pressable
              onPress={confirmRemove}
              accessibilityRole="button"
              accessibilityHint="The device leaves this home and must be paired again to come back"
              testID="remove-device"
              disabled={remove.isPending}
              style={{ alignSelf: 'center', paddingVertical: tokens.space.sm }}
            >
              <AppText variant="caption" tone="danger">
                {remove.isPending ? 'Removing…' : 'Remove this device'}
              </AppText>
            </Pressable>
            {remove.isError ? (
              <AppText variant="tiny" tone="danger" style={{ textAlign: 'center' }}>
                {remove.error instanceof Error ? remove.error.message : 'That device was not removed.'}
              </AppText>
            ) : null}
          </Box>
        ) : null}
      </Box>
    </Screen>
  );
}

/**
 * The points in the order a person reads them.
 *
 * `device.points` arrives sorted by code, so `battery.level` — a diagnostic number nobody opened this
 * screen for — came first and the temperature came last. Battery belongs at the end: it is about the
 * device, not about the room.
 */
function orderedPoints(points: DevicePoint[]): DevicePoint[] {
  const rank = ['temperature', 'humidity', 'energy', 'power', 'switch', 'brightness', 'level', 'battery'];
  const score = (point: DevicePoint) => {
    const at = rank.indexOf(point.capability);
    return at === -1 ? rank.length : at;
  };
  return [...points].sort((a, b) => score(a) - score(b));
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
  points,
  asOf,
}: {
  deviceId: string;
  points: DevicePoint[];
  /** When the data stops, for a device that has stopped: a chart drawn from silence has to say so. */
  asOf?: string | null;
}) {
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
  const charted = chartPoint(stored);
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
          {charted ? titleFor(charted) : code}
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
        // An empty chart is a dead end. The last value is the one fact we do have, and naming the
        // window makes "nothing here" a statement about a period rather than about the device.
        <AppText variant="caption" tone="muted">
          {charted && charted.value !== null && charted.value !== undefined
            ? `No readings in the last ${RANGE_WORDS[range]} — the latest is ${charted.value}${
                charted.unit ? ` ${charted.unit}` : ''
              }.`
            : `No readings in the last ${RANGE_WORDS[range]} yet.`}
        </AppText>
      ) : (
        <>
          <Sparkline buckets={data?.buckets ?? []} isBoolean={isBoolean} />
          {/* One string, not three children: a sentence assembled from nodes is also a sentence a test
              cannot match and a screen reader may split. The range is here because bars with no scale
              answer "did it move" and nothing else — the same complaint the hero's chart earned. */}
          {/* The range over the window, and nothing else: "284 readings · 26 bars" counted the platform's
              own plumbing — how many samples it stored and how many columns it drew — on a screen where the
              customer asked what the temperature has been doing. */}
          <AppText variant="caption" tone="muted">
            {`${chartRange(data?.buckets ?? [], charted)} over the last ${RANGE_WORDS[range]}`}
            {asOf ? ` · as of ${asOf}` : ''}
          </AppText>
        </>
      )}
    </Box>
  );
}

/** `21.3–21.9 °C · ` when there is a spread worth stating, otherwise nothing. */
function chartRange(
  buckets: { v: number | null }[],
  point?: { value?: unknown; unit?: string | null },
): string {
  const values = buckets.map((b) => b.v).filter((v): v is number => typeof v === 'number');
  // The live reading is part of the window. Leaving it out let the label contradict the number above
  // it — a maximum of 21.1 under a current 21.5 °C — which is the sort of thing a customer spots first.
  if (typeof point?.value === 'number') values.push(point.value);
  if (values.length < 2) return '';

  const unit = point?.unit ? ` ${point.unit}` : '';
  const low = Math.min(...values);
  const high = Math.max(...values);
  // A flat line is a fact too, and "21.5–21.5 °C" states it badly.
  if (Math.abs(high - low) < 0.05) return `steady at ${low.toFixed(1)}${unit}`;
  return `${low.toFixed(1)}–${high.toFixed(1)}${unit}`;
}
