import type { ReactNode } from 'react';
import { Pressable, StyleSheet, View, type ViewStyle } from 'react-native';

import { useTheme } from '../../app-shell/theme';
import { tokens } from '../../shared/config/tokens';

/**
 * A translucent surface with an optional accent halo.
 *
 * The halo is a second, offset layer rather than a shadow: `shadowColor` is iOS-only and on Android a
 * coloured shadow is not a thing, so a card that glowed on one platform would look flat on the other.
 * Two stacked views render identically on both, and on web.
 */
export function Glass({
  children,
  onPress,
  accent,
  glow,
  style,
  padded = true,
  disabled = false,
  accessibilityLabel,
  testID,
}: {
  children: ReactNode;
  onPress?: () => void;
  /** Category accent; tints the border and the halo. */
  accent?: string;
  glow?: string;
  style?: ViewStyle;
  padded?: boolean;
  disabled?: boolean;
  accessibilityLabel?: string;
  testID?: string;
}) {
  const { color, dark } = useTheme();

  const surface = (
    <View
      style={[
        {
          backgroundColor: dark ? color.glass : color.surface,
          borderRadius: tokens.radius.xl,
          borderWidth: StyleSheet.hairlineWidth,
          borderColor: accent ? withAlpha(accent, 0.32) : color.border,
          padding: padded ? tokens.space.lg : 0,
          overflow: 'hidden',
        },
        style,
      ]}
    >
      {children}
    </View>
  );

  const halo = accent ? (
    <View
      pointerEvents="none"
      style={{
        position: 'absolute',
        top: -2,
        left: -2,
        right: -2,
        bottom: -2,
        borderRadius: tokens.radius.xl + 2,
        backgroundColor: glow ?? withAlpha(accent, 0.12),
        opacity: dark ? 0.9 : 0.55,
      }}
    />
  ) : null;

  if (!onPress) {
    return (
      <View testID={testID} accessibilityLabel={accessibilityLabel}>
        {halo}
        {surface}
      </View>
    );
  }

  return (
    <Pressable
      testID={testID}
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel}
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => ({ opacity: pressed ? 0.85 : disabled ? 0.6 : 1 })}
    >
      {halo}
      {surface}
    </Pressable>
  );
}

/** `#RRGGBB` + alpha, so one accent can tint a border, a halo and a chart fill. */
export function withAlpha(hex: string, alpha: number): string {
  const clean = hex.replace('#', '');
  if (clean.length !== 6) return hex;
  const r = parseInt(clean.slice(0, 2), 16);
  const g = parseInt(clean.slice(2, 4), 16);
  const b = parseInt(clean.slice(4, 6), 16);
  return `rgba(${r},${g},${b},${alpha})`;
}
