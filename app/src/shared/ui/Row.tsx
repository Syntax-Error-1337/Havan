import { Pressable, Switch as RNSwitch, View } from 'react-native';
import type { ReactNode } from 'react';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText } from './Box';

/**
 * A row in a grouped section: a label, an optional second line, and one control
 * on the right — a switch, a value, or a chevron.
 *
 * 44 pt minimum height is not a style choice; it is the smallest target a finger
 * hits reliably, and a list of 32 pt rows is a list people mis-tap.
 */
export function Row({
  title,
  subtitle,
  value,
  onPress,
  right,
  disabled,
}: {
  title: string;
  subtitle?: string;
  /** The trailing text a value row shows (an iOS "detail" cell). */
  value?: string;
  onPress?: () => void;
  right?: ReactNode;
  disabled?: boolean;
}) {
  const { color } = useTheme();

  const body = (
    <View
      style={{
        minHeight: tokens.touch.min,
        flexDirection: 'row',
        alignItems: 'center',
        gap: tokens.space.md,
        paddingHorizontal: tokens.space.lg,
        paddingVertical: tokens.space.md,
        opacity: disabled ? 0.4 : 1,
      }}
    >
      <View style={{ flex: 1 }}>
        <AppText numberOfLines={1}>{title}</AppText>
        {subtitle ? (
          <AppText variant="caption" tone="muted" numberOfLines={1}>
            {subtitle}
          </AppText>
        ) : null}
      </View>

      {value ? (
        <AppText variant="body" tone="muted">
          {value}
        </AppText>
      ) : null}

      {right}

      {onPress && !right ? (
        // A chevron, drawn rather than shipped as an icon font: one glyph is not
        // worth a dependency.
        <AppText variant="subhead" tone="muted" style={{ marginLeft: tokens.space.xs }}>
          ›
        </AppText>
      ) : null}
    </View>
  );

  if (!onPress) return body;

  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityRole="button"
      accessibilityLabel={subtitle ? `${title}, ${subtitle}` : title}
      accessibilityState={{ disabled: Boolean(disabled) }}
      style={({ pressed }) => ({ backgroundColor: pressed ? color.elevated : 'transparent' })}
    >
      {body}
    </Pressable>
  );
}

/** The system switch, tinted with the app accent. */
export function Toggle({
  value,
  onChange,
  disabled,
  label,
}: {
  value: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
  label: string;
}) {
  const { color } = useTheme();
  return (
    <RNSwitch
      value={value}
      onValueChange={onChange}
      disabled={disabled}
      accessibilityLabel={label}
      trackColor={{ true: color.accent, false: color.elevated }}
      // iOS leaves the thumb white on both tracks in dark mode.
      thumbColor="#FFFFFF"
      ios_backgroundColor={color.elevated}
    />
  );
}
