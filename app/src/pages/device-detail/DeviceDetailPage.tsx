import { useState } from 'react';
import { ActivityIndicator, Pressable, ScrollView, StyleSheet, Switch, TextInput, View } from 'react-native';
import { useLocalSearchParams, useRouter } from 'expo-router';
import {
  Box,
  Screen,
  AppText,
  Card,
  DeviceArtField,
  Icon,
  AreaChart,
  iconForCategory,
  QueryError,
  Row,
  Section,
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
import { useToast } from '../../shared/ui/Toast';
import { useInlineEdit } from '../../shared/lib/useInlineEdit';
import { reading, valueWithUnit } from '../../shared/ui/units';
import { chartBand } from '../../entities/device/model/chart';

export function DeviceDetailPage() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const { color, dark } = useTheme();
  const toast = useToast();
  const { data: device, isPending, isError, error, refetch } = useDevice(id);
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
    if (id) rename.mutate({ deviceId: id, name }, { onSuccess: () => toast(`Renamed to ${name}`) });
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
        <QueryError
          title="Device unavailable"
          error={error}
          onRetry={() => void refetch()}
        />
      </Screen>
    );
  }

  return (
    <Screen>
      {sheet}
      {/* A scroll view, because a phone is not a browser.
          The web build lets the *document* scroll, so everything below 844 points was reachable in the browser
          and unreachable on iOS — including "Remove this device", which sat at y=872. */}
      <ScrollView
        contentContainerStyle={{ padding: tokens.space.lg, paddingBottom: tokens.layout.tabBarClearance, gap: tokens.space.lg }}
        showsVerticalScrollIndicator={false}
        // The rename field lives in here; without this a tap on a room chip while renaming only dismisses.
        keyboardShouldPersistTaps="handled"
      >
        {/* Every screen that was pushed onto the stack needs a way back off it, and this one had none: the
            only exits were the tab bar (which loses your place) and the platform's own gesture, which a
            screenshot cannot see and a desktop browser does not have. */}
        <Pressable
          onPress={() => (router.canGoBack() ? router.back() : router.replace('/'))}
          accessibilityRole="button"
          accessibilityLabel="Back"
          accessibilityHint="Returns to the previous screen"
          testID="device-back"
          style={{
            width: tokens.touch.min,
            height: tokens.touch.min,
            borderRadius: tokens.touch.min / 2,
            alignItems: 'center',
            justifyContent: 'center',
            backgroundColor: color.glass,
            borderWidth: StyleSheet.hairlineWidth,
            borderColor: color.border,
            marginBottom: -tokens.space.sm,
          }}
        >
          <View style={{ transform: [{ rotate: '180deg' }] }}>
            <Icon name="chevron-right" size={20} color={color.text} />
          </View>
        </Pressable>
        {/* The hero is the device's own category: a bulb glows violet, a lock pink, a camera blue —
            chosen from data, so a product we have never seen still looks deliberate. */}
        <Card accent={accent} level={2} padded={false}>
          {/* The tile this device was opened from has a field behind its reading. The detail screen is the same
              object, larger — so it keeps the field, at the top, where the name and the reading are. */}
          <View pointerEvents="none" style={{ position: 'absolute', left: 0, right: 0, top: 0 }}>
            <DeviceArtField
              accent={accent}
              glyph={iconForCategory(device.category)}
              height={150}
              dimmed={freshness(device) !== 'live'}
              style={{ borderRadius: 0 }}
            />
          </View>
          <View style={{ padding: 20 }}>
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
              // The name is a real control (it opens the editor); a 28-point line of type needs a target.
              style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
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
          </View>
        </Card>

        {/* Values below are the last ones this device reported. Saying so is the difference between
            "this is broken" and "this is what the house looked like when it went quiet". */}
        {freshness(device) !== 'live' ? (
          <Card accent={color.warning}>
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
          </Card>
        ) : null}

        <HistoryChart
          deviceId={device.id}
          accent={accent}
          gapColor={color.muted}
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
        {/* `opacity: 0.6` over the controls was the same defect found on the dashboard: it dims the labels
            with the switches, and "Offline" in 60 % grey is the one sentence the customer needs to read.
            The controls say they are inert themselves (a disabled fill, a greyed label); this only stops
            them being tappable. */}
        <Box gap="md" pointerEvents={device.online ? 'auto' : 'none'}>
          {orderedPoints(device.points).map((point) => (
            <Card key={point.code} accent={point.writable ? accent : undefined}>
              <PointControl device={device} point={point} accent={accent} />
            </Card>
          ))}
        </Box>

        {setPoint.error ? (
          <AppText tone="danger" variant="caption">
            {setPoint.error.message}
          </AppText>
        ) : null}

        {/* The facts a support call starts with, which this screen had none of: the serial printed on the box, what
            product it is, what firmware it runs and when it last said anything. All of it was already in the payload —
            it was simply never shown, so "what is this device" could only be answered by the vendor's portal. */}
        <Section title="About this device">
          <Row title="Serial" value={device.serial || '—'} />
          <Row title="Product" value={device.product_key || '—'} />
          <Row title="Firmware" value={device.fw_version ?? 'Not reported'} />
          <Row
            title="Last reported"
            value={device.last_seen_at ? relative(device.last_seen_at) : 'Never'}
          />
          <Row
            title="Reports every"
            value={
              device.report_interval_secs ? `${device.report_interval_secs} s` : 'Not declared'
            }
          />
        </Section>

        {/* Configuration last. This used to sit between the offline banner and the readings, so the
            first thing under "this device has stopped reporting" was a list of rooms — a setting nobody
            opened the screen to change, in front of the numbers they did. */}
        <Box gap="xs">
          <AppText variant="caption" tone="muted">
            In which room
          </AppText>
          {/* One scrolling row rather than a wrapping one: five rooms wrapped to two lines with "Kitchen"
              alone underneath, which reads as a mistake rather than a list you can swipe. */}
          <ScrollView
            horizontal
            showsHorizontalScrollIndicator={false}
            // The insets are the screen's own gutter, on both sides: without them the first chip starts against the edge
            // and the last one is cut mid-word by it, which reads as a clipped control rather than a row you can swipe.
            // The negative margins let that inset live inside the ScrollView instead of shrinking the row.
            style={{ marginHorizontal: -tokens.space.lg }}
            contentContainerStyle={{
              gap: tokens.space.sm,
              paddingHorizontal: tokens.space.lg,
              paddingVertical: 2,
            }}
          >
            {[{ id: null as string | null, name: 'Unassigned' }, ...(rooms ?? [])].map((room) => {
              const active = (device.room_id ?? null) === room.id;
              return (
                <Pressable
                  key={room.id ?? 'none'}
                  disabled={setRoom.isPending}
                  onPress={() =>
                    setRoom.mutate(
                      { deviceId: device.id, roomId: room.id },
                      {
                        onSuccess: () =>
                          toast(room.id ? `Moved to ${room.name}` : 'Moved out of its room'),
                      },
                    )
                  }
                  accessibilityRole="button"
                  accessibilityState={{ selected: active }}
                  style={{ minHeight: tokens.touch.min, justifyContent: 'center' }}
                >
                  <Box
                    px="md"
                    py="sm"
                    radius="pill"
                    style={{
                      // `glass` + muted text made every room you did *not* pick look disabled — the same
                      // defect the scene editor's chips had. An unselected option is still a live target.
                      backgroundColor: active ? color.accent : color.surface,
                      borderWidth: StyleSheet.hairlineWidth,
                      borderColor: active ? color.accent : color.border,
                    }}
                  >
                    <AppText variant="caption" style={{ color: active ? color.onAccent : color.text }}>
                      {room.name}
                    </AppText>
                  </Box>
                </Pressable>
              );
            })}
          </ScrollView>
          {setRoom.error ? (
            <AppText variant="caption" tone="danger">
              {setRoom.error.message}
            </AppText>
          ) : null}
        </Box>


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
              style={{ alignSelf: 'center', paddingVertical: tokens.space.sm, minHeight: tokens.touch.min, justifyContent: 'center' }}
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
      </ScrollView>
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
  accent,
  gapColor,
  points,
  asOf,
}: {
  deviceId: string;
  accent: string;
  gapColor: string;
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

  // The band describes the drawing: measured buckets, plus the live reading *only* when it is part of the
  // window. A stale device's current value is a different observation from another moment, and mixing it in
  // made the sentence under the chart disagree with the numbers printed at its edges.
  const liveValue = typeof charted?.value === 'number' && !asOf ? charted.value : null;
  const band = chartBand(data?.buckets ?? [], charted?.unit, liveValue);

  return (
    <Box gap="xs">
      {/* `flexWrap`: at 200% zoom "Temperature" and four 44-point chips do not share a 195-point row, and a
          row that refuses to wrap pushes the last chip 82 points off the screen. */}
      <Box row style={{ justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 8 }}>
        <AppText variant="caption" tone="muted">
          {charted ? titleFor(charted) : code}
        </AppText>
        {/* Four 44-point chips and their gaps need 200 points; a 195-point row has 179. Wrapping the chips
            themselves is the difference between the last one being reachable and being 9 points off screen. */}
        <Box row gap="xs" style={{ flexWrap: 'wrap', flexShrink: 1, maxWidth: '100%' }}>
          {RANGES.map((option) => (
            <Pressable
              key={option}
              onPress={() => setRange(option)}
              accessibilityRole="button"
              accessibilityState={{ selected: range === option }}
              // `1h` is two characters and 30 points wide; the target is the minimum, not the glyph.
              style={{ minHeight: tokens.touch.min, minWidth: tokens.touch.min, justifyContent: 'center' }}
            >
              <Box px="sm" py="xs" radius="pill" bg={range === option ? 'accent' : 'surface'}>
                {/* Colour alone is not a state: the selected range also carries the heavier weight, so it
                    survives a greyscale screenshot, a colour-blind eye and a printed support ticket. */}
                <AppText
                  variant="caption"
                  weight={range === option ? '700' : '500'}
                  tone={range === option ? 'text' : 'muted'}
                >
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
            ? // `reading()` too: the note said "the latest is 18.27 °C" beside a card reading "18.3 °C", which
              // is two numbers for one temperature — and the customer checks the pair before anything else.
              `No readings in the last ${RANGE_WORDS[range]} — the latest is ${valueWithUnit(
                typeof charted.value === 'number' ? reading(charted.value) : String(charted.value),
                charted.unit,
              )}.`
            : // The range chips above an empty chart read as an orphaned control; this is what they are for. Naming
              // the way out is cheaper than explaining the control.
              `No readings in the last ${RANGE_WORDS[range]} yet${range === '30d' ? '.' : ' — try a longer range.'}`}
        </AppText>
      ) : (
        <>
          {/* A line, and the same one the dashboard draws.
              This was a bar sparkline whose scale started at **zero** — so a day that moved between
              16.6 and 21.5 °C drew as twenty-six bars of 0.77, 0.80, 0.83 … all the same height. The
              screen's one job is "what has it been doing", and it answered "it has been a rectangle".
              Bars encode magnitude and so must start at zero; a line encodes change, and may describe a
              band — which is why the range is printed underneath: the axis is truncated, and says so. */}
          <Box style={{ paddingVertical: tokens.space.sm }}>
            <AreaChart
              buckets={data?.buckets ?? []}
              accent={accent}
              gapColor={gapColor}
              height={120}
              scale={
                band
                  ? { min: band.min, max: band.max, color: tokens.chart.axis }
                  : null
              }
              // The window the axis draws, not the extent of the data: labelling the first and last bucket made
              // a 24-hour axis read "7 AM … 11 AM" because the sensor had only reported for four of those hours.
              axis={{
                from: axisLabel(data?.from, range),
                // The right end is *now*: the chart's axis runs to the present, which is what makes the empty
                // stretch after the last reading read as time that passed. `data.to` is the last bucket, so
                // using it labelled both ends of the axis with the same hour.
                to: axisLabel(new Date().toISOString(), range),
                color: tokens.chart.axis,
              }}
            />
          </Box>
          {/* One string, not three children: a sentence assembled from nodes is also a sentence a test
              cannot match and a screen reader may split. The range is here because bars with no scale
              answer "did it move" and nothing else — the same complaint the hero's chart earned. */}
          {/* The range over the window, and nothing else: "284 readings · 26 bars" counted the platform's
              own plumbing — how many samples it stored and how many columns it drew — on a screen where the
              customer asked what the temperature has been doing. */}
          <AppText variant="caption" tone="muted">
            {`${band ? band.text : ''}${band ? ' ' : ''}over the last ${RANGE_WORDS[range]}`}
            {asOf ? ` · last reading ${asOf}` : ''}
          </AppText>
        </>
      )}
    </Box>
  );
}

/**
 * One end of the time axis, in the customer's own clock.
 *
 * An hour window is told in hours (`11 PM`), a week or a month in days (`Mon`, `Nov 3`) — printing "11 PM" at
 * both ends of a month-long axis is a label that describes nothing.
 */
function axisLabel(iso: string | undefined, range: Range): string {
  if (!iso) return '';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '';
  if (range === '1h') return d.toLocaleTimeString(undefined, { hour: 'numeric' });
  // A day's axis can span two calendar days, and two labels that both read "7 AM" describe nothing.
  if (range === '1d') return d.toLocaleString(undefined, { weekday: 'short', hour: 'numeric' });
  return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}
