import { ActivityIndicator, Pressable, type PressableProps } from 'react-native';
import { useTheme, tokens } from '../../app-shell/theme';
import { AppText } from './Box';

export type ButtonProps = Omit<PressableProps, 'children'> & {
  label: string;
  variant?: 'primary' | 'ghost' | 'danger';
  loading?: boolean;
  full?: boolean;
};

export function Button({
  label,
  variant = 'primary',
  loading = false,
  full = false,
  disabled,
  style,
  ...rest
}: ButtonProps) {
  const { color } = useTheme();
  const isDisabled = disabled || loading;
  // `ghost` is a bar button: iOS puts navigation actions in the tint colour with
  // no border and no fill. A row of outlined pills is a web toolbar, and it makes
  // every action compete with the one real action on the screen.
  // A disabled button must look disabled. Half-opacity on a filled accent is still a blue button on a dark
  // screen — iOS greys the fill and the label instead, and grey is the only thing that reads as "not yet".
  const bg = isDisabled
    ? variant === 'ghost'
      ? 'transparent'
      : color.glass
    : variant === 'primary'
      ? color.accent
      : variant === 'danger'
        ? color.danger
        : 'transparent';
  const fg = isDisabled
    ? color.muted
    : variant === 'primary'
      ? color.onAccent
      : variant === 'danger'
        ? color.dangerOn
        : color.accent;

  return (
    <Pressable
      accessibilityRole="button"
      accessibilityState={{ disabled: Boolean(isDisabled), busy: loading }}
      disabled={isDisabled}
      style={[
        {
          minHeight: tokens.touch.min,
          paddingHorizontal: variant === 'ghost' ? tokens.space.md : tokens.space.xl,
          borderRadius: tokens.radius.md,
          backgroundColor: bg,
          borderWidth: 0,
          borderColor: color.border,
          alignItems: 'center',
          justifyContent: 'center',
          flexDirection: 'row',
          gap: tokens.space.sm,
          opacity: isDisabled && variant === 'ghost' ? 0.55 : 1,
          // A button is as wide as its label — until the label does not fit. Without these two the label
          // took the button past the edge of a 195-point screen ("Create your first rule" at 200% zoom),
          // where it is not reachable at all. The label wraps instead.
          flexShrink: 1,
          maxWidth: '100%',
        },
        full && { alignSelf: 'stretch' },
        style as object,
      ]}
      {...rest}
    >
      {loading ? <ActivityIndicator color={fg} /> : null}
      <AppText weight="600" style={{ color: fg, flexShrink: 1, textAlign: 'center' }}>
        {label}
      </AppText>
    </Pressable>
  );
}
