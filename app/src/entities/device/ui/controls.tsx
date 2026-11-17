import { useMemo, useRef, useState } from 'react';
import { PanResponder, Pressable, StyleSheet, View, type LayoutChangeEvent } from 'react-native';
import * as Haptics from 'expo-haptics';

import { useTheme, tokens } from '../../../app-shell/theme';
import { AppText } from '../../../shared/ui';
import { withAlpha } from '../../../shared/ui/Glass';
import { useSetPoint } from '../model/queries';
import { commandNotice } from '../model/notices';
import type { Device, DevicePoint } from '../model/types';

/**
 * What a point *is*, read from its own data.
 *
 * This is the whole answer to "any vendor's product should get the right control". The thing model
 * already says which capability a point belongs to, whether it is writable, and what its type and
 * range are — so the app chooses a toggle for a bool, a slider for a bounded number, a stepper for a
 * temperature, and a readout for anything a person cannot set. No per-product code, and a product
 * shipped tomorrow gets a control today.
 */
export type ControlKind = 'toggle' | 'slider' | 'stepper' | 'readout';

export function controlFor(point: DevicePoint): ControlKind {
  if (!point.writable) return 'readout';
  if (point.kind === 'bool') return 'toggle';
  const bounded = point.min !== null && point.min !== undefined && point.max !== null && point.max !== undefined;
  if (point.kind === 'enum') return 'stepper';
  if (bounded) {
    // A temperature is adjusted in whole degrees, not dragged across a range — a person says "21",
    // not "20.6". Everything else that is bounded is a level and deserves the slider.
    const code = point.code.toLowerCase();
    if (code.includes('temperature') || code.includes('temp')) return 'stepper';
    return 'slider';
  }
  return 'stepper';
}

/** A tactile tick on a change the platform has accepted. Silent on web, where there is no motor. */
function tick() {
  Haptics.selectionAsync().catch(() => {});
}

/**
 * A horizontal level control.
 *
 * Hand-rolled because React Native removed `Slider` and the community package needs a native module
 * that does not exist on web — and this app is verified in a browser before it is verified on a
 * phone. PanResponder works on both. The track is tall enough (28 pt) that a thumb-sized finger can
 * grab it anywhere, which matters more than the visual thumb size.
 */
function Slider({
  value,
  min,
  max,
  accent,
  disabled,
  onChange,
  label,
}: {
  value: number;
  min: number;
  max: number;
  accent: string;
  disabled?: boolean;
  onChange: (next: number) => void;
  label: string;
}) {
  const { color } = useTheme();
  const [width, setWidth] = useState(0);
  const [preview, setPreview] = useState<number | null>(null);
  const widthRef = useRef(0);

  const span = Math.max(1, max - min);
  const shown = preview ?? value;
  const pct = Math.min(1, Math.max(0, (shown - min) / span));

  const commit = useMemo(() => {
    const at = (x: number) => Math.round(min + Math.min(1, Math.max(0, x / Math.max(1, widthRef.current))) * span);
    return { at };
  }, [min, span]);

  const responder = useMemo(
    () =>
      PanResponder.create({
        onStartShouldSetPanResponder: () => !disabled,
        onMoveShouldSetPanResponder: () => !disabled,
        onPanResponderGrant: (e) => setPreview(commit.at(e.nativeEvent.locationX)),
        onPanResponderMove: (e) => setPreview(commit.at(e.nativeEvent.locationX)),
        onPanResponderRelease: (e) => {
          const next = commit.at(e.nativeEvent.locationX);
          setPreview(null);
          if (next !== value) {
            tick();
            onChange(next);
          }
        },
        onPanResponderTerminate: () => setPreview(null),
      }),
    [commit, disabled, onChange, value],
  );

  const onLayout = (e: LayoutChangeEvent) => {
    widthRef.current = e.nativeEvent.layout.width;
    setWidth(e.nativeEvent.layout.width);
  };

  return (
    <View
      {...responder.panHandlers}
      onLayout={onLayout}
      accessibilityRole="adjustable"
      accessibilityLabel={label}
      accessibilityValue={{ min, max, now: shown }}
      style={{ height: tokens.touch.min, justifyContent: 'center', opacity: disabled ? 0.5 : 1 }}
    >
      <View
        style={{
          height: 10,
          borderRadius: 5,
          backgroundColor: withAlpha(color.text, 0.1),
          overflow: 'hidden',
        }}
      >
        <View
          style={{
            width: `${pct * 100}%`,
            height: '100%',
            borderRadius: 5,
            backgroundColor: accent,
          }}
        />
      </View>
      {width > 0 ? (
        <View
          pointerEvents="none"
          style={{
            position: 'absolute',
            left: Math.max(0, Math.min(width - 18, pct * width - 9)),
            width: 18,
            height: 18,
            borderRadius: 9,
            backgroundColor: color.thumb,
            borderWidth: 2,
            borderColor: accent,
          }}
        />
      ) : null}
    </View>
  );
}

/** A stepper for values a person names rather than drags: "+ 21 °C −". */
function Stepper({
  value,
  min,
  max,
  step,
  accent,
  unit,
  disabled,
  onChange,
  label,
}: {
  value: number;
  min: number;
  max: number;
  step: number;
  accent: string;
  unit?: string | null;
  disabled?: boolean;
  onChange: (next: number) => void;
  label: string;
}) {
  const { color } = useTheme();
  const clamp = (n: number) => Math.min(max, Math.max(min, n));
  const round = (n: number) => Math.round(n * 10) / 10;
  const at = round(value);

  const button = (dir: -1 | 1, glyph: string) => (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${label} ${dir > 0 ? 'increase' : 'decrease'}`}
      disabled={disabled || (dir > 0 ? at >= max : at <= min)}
      onPress={() => {
        tick();
        onChange(round(clamp(at + dir * step)));
      }}
      style={({ pressed }) => ({
        width: tokens.touch.min,
        height: tokens.touch.min,
        borderRadius: tokens.touch.min / 2,
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: withAlpha(accent, pressed ? 0.28 : 0.16),
        // 0.4 made a disabled stepper look like a rendering fault rather than a refused control;
        // 0.55 still reads as unavailable and still reads as a button.
        opacity: disabled ? 0.55 : 1,
      })}
    >
      <AppText variant="title" weight="600" style={{ color: accent, lineHeight: 24 }}>
        {glyph}
      </AppText>
    </Pressable>
  );

  return (
    // Equal space on both sides of the value, so the control spans the card rather than hanging off
    // its left edge with a hole to the right.
    <View style={{ flexDirection: 'row', alignItems: 'center', gap: tokens.space.sm }}>
      {button(-1, '−')}
      <AppText
        variant="title"
        weight="600"
        style={{ color: accent, flex: 1, textAlign: 'center' }}
      >
        {at}
        {unit ? <AppText variant="caption" tone="muted">{` ${unit}`}</AppText> : null}
      </AppText>
      {button(1, '+')}
    </View>
  );
}

/** A pill switch that reads as a device control rather than as a settings toggle. */
export function PillSwitch({
  on,
  accent,
  onChange,
  disabled,
  label,
}: {
  on: boolean;
  accent: string;
  onChange: () => void;
  disabled?: boolean;
  label: string;
}) {
  const { color } = useTheme();
  return (
    <Pressable
      accessibilityRole="switch"
      accessibilityState={{ checked: on, disabled }}
      accessibilityLabel={label}
      disabled={disabled}
      onPress={() => {
        tick();
        onChange();
      }}
      style={{
        width: 58,
        height: 34,
        borderRadius: 17,
        padding: 3,
        justifyContent: 'center',
        // An offline device cannot act, so its control must not wear the "on" colour: a vivid
        // switch that does nothing when pressed is the worse of the two lies.
        backgroundColor: disabled ? withAlpha(color.text, 0.1) : on ? accent : withAlpha(color.text, 0.14),
        opacity: disabled ? 0.38 : 1,
      }}
    >
      <View
        style={{
          width: 28,
          height: 28,
          borderRadius: 14,
          backgroundColor: color.thumb,
          alignSelf: on ? 'flex-end' : 'flex-start',
          ...StyleSheet.flatten([{ transform: [{ translateX: 0 }] }]),
        }}
      />
    </Pressable>
  );
}

/**
 * One point, with the control its own data calls for.
 *
 * Every control talks to the same mutation, so a device the app has never seen still produces a
 * command the platform understands: the point's `code` is the address on the wire.
 */
export function PointControl({
  device,
  point,
  accent,
}: {
  device: Device;
  point: DevicePoint;
  accent: string;
}) {
  const { color } = useTheme();
  const setPoint = useSetPoint(device.id);
  const kind = controlFor(point);
  const disabled = !device.online || setPoint.isPending;
  const label = `${device.name} ${point.code}`;

  const set = (value: boolean | number) => setPoint.mutate({ code: point.code, value });

  return (
    <View style={{ gap: tokens.space.xs }}>
      <View style={{ flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: tokens.space.md }}>
        <View style={{ flex: 1 }}>
          <AppText variant="subhead" numberOfLines={1}>
            {titleFor(point)}
          </AppText>
          <AppText variant="caption" tone="muted" numberOfLines={1}>
            {point.code}
            {point.unit ? ` · ${point.unit}` : ''}
          </AppText>
        </View>

        {kind === 'toggle' ? (
          <View style={{ flexDirection: 'row', alignItems: 'center', gap: tokens.space.sm }}>
            {/* The knob's position is a convention; the word is not. On a disabled control the
                convention alone reads as "maybe on". */}
            <AppText variant="caption" tone="muted">
              {point.value === true ? 'On' : 'Off'}
            </AppText>
            <PillSwitch
              on={point.value === true}
              accent={accent}
              disabled={disabled}
              label={label}
              onChange={() => set(point.value !== true)}
            />
          </View>
        ) : null}

        {point.kind !== 'bool' && kind === 'slider' ? (
          <AppText variant="title" weight="600" style={{ color: accent }}>
            {String(point.value ?? point.min ?? 0)}
            {point.unit ? <AppText variant="caption" tone="muted">{` ${point.unit}`}</AppText> : null}
          </AppText>
        ) : null}

        {kind === 'readout' ? (
          <AppText variant="title" weight="600">
            {point.value === null || point.value === undefined
              ? '—'
              : typeof point.value === 'boolean'
                ? point.value
                  ? 'Yes'
                  : 'No'
                : `${point.value}${point.unit ? ` ${point.unit}` : ''}`}
          </AppText>
        ) : null}
      </View>

      {kind === 'slider' ? (
        <Slider
          value={typeof point.value === 'number' ? point.value : (point.min ?? 0)}
          min={point.min ?? 0}
          max={point.max ?? 100}
          accent={accent}
          disabled={disabled}
          label={label}
          onChange={(v) => set(v)}
        />
      ) : null}

      {kind === 'stepper' ? (
        <Stepper
          value={typeof point.value === 'number' ? point.value : (point.min ?? 0)}
          min={point.min ?? 0}
          max={point.max ?? 100}
          step={1}
          unit={point.unit ?? null}
          accent={accent}
          disabled={disabled}
          label={label}
          onChange={(v) => set(v)}
        />
      ) : null}

      {setPoint.isError ? (
        <AppText variant="caption" style={{ color: color.danger }}>
          {commandNotice(setPoint.error)}
        </AppText>
      ) : null}
    </View>
  );
}

/**
 * Capabilities that name a *kind of device* rather than a reading.
 *
 * A humidity sensor's capability is `humidity` and its slot is `value`; a bulb's capability is `switch`
 * and its slot is `power`. Taking the capability first names the first correctly and the second
 * uselessly, so the generic ones defer to the slot.
 */
const DEVICE_KIND_CAPABILITIES = new Set([
  'switch',
  'sensor',
  'plug',
  'light',
  'cover',
  'lock',
  'climate',
  'camera',
  'speaker',
  'vacuum',
  'energy',
]);

/**
 * The heading a person reads above a control: "Temperature", not "Value".
 *
 * It used to prefer the slot, so a humidity sensor and a temperature sensor both appeared as "Value" —
 * two cards on the same screen with the same heading and different numbers, which is the sort of thing
 * that makes an app feel unfinished without anyone being able to say why.
 */
export function titleFor(point: { capability?: string; slot?: string; code: string }): string {
  const capability = point.capability;
  if (capability && !DEVICE_KIND_CAPABILITIES.has(capability)) return humanise(capability);
  return humanise(point.slot || capability || point.code);
}

/** `brightness.level` → `Level`, `switch.power` → `Power`. Labels are for people, codes are for the wire. */
function humanise(slot: string): string {
  const last = slot.includes('.') ? slot.split('.').pop()! : slot;
  return last
    .replace(/[_-]+/g, ' ')
    .replace(/^./, (c) => c.toUpperCase());
}
