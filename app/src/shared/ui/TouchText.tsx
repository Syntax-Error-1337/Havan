import type { ReactNode } from 'react';
import { Pressable, type StyleProp, type ViewStyle } from 'react-native';

import { tokens } from '../../app-shell/theme';

/**
 * A text control that is actually a target.
 *
 * This exists because the same defect has now been found five times by the same gate: a `Pressable` wrapped around a
 * `<AppText variant="caption">` measures about 50 × 18, and every one of those was reported as "target smaller than
 * 44px" only *after* somebody opened the screen with data on it. The minimums belong in one place, not in every call
 * site's memory.
 *
 * Both dimensions, always: width is the one that surprises people — "History" is wide enough until it becomes "Hide",
 * and a label that changes length changes the target.
 */
export function TouchText({
  children,
  onPress,
  label,
  hint,
  disabled,
  destructive,
  testID,
  style,
}: {
  children: ReactNode;
  onPress: () => void;
  /** What a screen reader hears. Required: a control with no name is the other half of the same gate. */
  label: string;
  hint?: string;
  disabled?: boolean;
  destructive?: boolean;
  testID?: string;
  style?: StyleProp<ViewStyle>;
}) {
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityRole="button"
      accessibilityLabel={label}
      accessibilityHint={hint}
      accessibilityState={{ disabled: Boolean(disabled) }}
      testID={testID}
      style={[
        {
          minWidth: tokens.touch.min,
          minHeight: tokens.touch.min,
          alignItems: 'center',
          justifyContent: 'center',
          // A destructive word is not the place to save eight points of width.
          paddingHorizontal: destructive ? tokens.space.xs : 0,
        },
        style,
      ]}
    >
      {children}
    </Pressable>
  );
}
