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
  const bg =
    variant === 'primary' ? color.accent : variant === 'danger' ? color.danger : 'transparent';
  const fg =
    variant === 'primary' ? color.onAccent : variant === 'danger' ? '#FFFFFF' : color.accent;

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
          opacity: isDisabled ? 0.5 : 1,
        },
        full && { alignSelf: 'stretch' },
        style as object,
      ]}
      {...rest}
    >
      {loading ? <ActivityIndicator color={fg} /> : null}
      <AppText weight="600" style={{ color: fg }}>
        {label}
      </AppText>
    </Pressable>
  );
}
