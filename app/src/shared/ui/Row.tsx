import { Pressable, Switch as RNSwitch, View } from 'react-native';
import type { ReactNode } from 'react';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText } from './Box';

/**
 * A row in a grouped section: a label, an optional second line, and one control on
 * the right — a switch, a value, or a chevron.
 *
 * The label area is the pressable part and the control sits *outside* it. Wrapping
 * both in one Pressable is the obvious implementation and the wrong one: a switch or
 * a Delete button inside a pressable row is a button inside a button, which the web
 * renderer rejects outright and which makes the tap ambiguous everywhere else. It is
 * also what a `UITableViewCell` does — the accessory is a sibling, not a child.
 *
 * 44 pt minimum height is not a style choice; it is the smallest target a finger hits
 * reliably, and a list of 32 pt rows is a list people mis-tap.
 */
export function Row({
  title,
  subtitle,
  value,
  onPress,
  right,
  disabled,
  destructive,
}: {
  title: string;
  subtitle?: string;
  /** The trailing text a value row shows (an iOS "detail" cell). */
  value?: string;
  onPress?: () => void;
  right?: ReactNode;
  disabled?: boolean;
  /** A row that ends something (sign out, delete) — red, as iOS does it. */
  destructive?: boolean;
}) {
  const { color } = useTheme();

  const label = (
    <View
      style={{
        flex: 1,
        minHeight: tokens.touch.min,
        justifyContent: 'center',
        paddingVertical: tokens.space.md,
      }}
    >
      <AppText numberOfLines={1} style={destructive ? { color: color.danger } : undefined}>
        {title}
      </AppText>
      {subtitle ? (
        <AppText variant="caption" tone="muted" numberOfLines={1}>
          {subtitle}
        </AppText>
      ) : null}
    </View>
  );

  return (
    <View
      style={{
        flexDirection: 'row',
        alignItems: 'center',
        gap: tokens.space.md,
        paddingHorizontal: tokens.space.lg,
        opacity: disabled ? 0.4 : 1,
      }}
    >
      {onPress ? (
        <Pressable
          onPress={onPress}
          disabled={disabled}
          accessibilityRole="button"
          accessibilityLabel={subtitle ? `${title}, ${subtitle}` : title}
          accessibilityState={{ disabled: Boolean(disabled) }}
          style={({ pressed }) => ({
            flex: 1,
            flexDirection: 'row',
            alignItems: 'center',
            gap: tokens.space.md,
            backgroundColor: pressed ? color.elevated : 'transparent',
          })}
        >
          {label}
          {value ? (
            <AppText variant="body" tone="muted">
              {value}
            </AppText>
          ) : null}
          {!right && !destructive ? (
            // A chevron, drawn rather than shipped as an icon font: one glyph is not
            // worth a dependency. A destructive row has none — a chevron says "this
            // goes somewhere", and signing out does not.
            <AppText variant="subhead" tone="muted">
              ›
            </AppText>
          ) : null}
        </Pressable>
      ) : (
        <View style={{ flex: 1, flexDirection: 'row', alignItems: 'center', gap: tokens.space.md }}>
          {label}
          {value ? (
            <AppText variant="body" tone="muted">
              {value}
            </AppText>
          ) : null}
        </View>
      )}

      {right}
    </View>
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
