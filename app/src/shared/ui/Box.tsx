import { Platform, StyleSheet, Text, View, type TextProps, type ViewProps } from 'react-native';
import { useTheme, tokens } from '../../app-shell/theme';

type Space = keyof typeof tokens.space;

export type BoxProps = ViewProps & {
  p?: Space;
  px?: Space;
  py?: Space;
  gap?: Space;
  bg?: keyof typeof tokens.color;
  radius?: keyof typeof tokens.radius;
  border?: boolean;
  flex?: number;
  row?: boolean;
  center?: boolean;
};

export function Box({
  p,
  px,
  py,
  gap,
  bg,
  radius,
  border,
  flex,
  row,
  center,
  style,
  ...rest
}: BoxProps) {
  const { color } = useTheme();
  return (
    <View
      style={[
        p !== undefined && { padding: tokens.space[p] },
        px !== undefined && { paddingHorizontal: tokens.space[px] },
        py !== undefined && { paddingVertical: tokens.space[py] },
        gap !== undefined && { gap: tokens.space[gap] },
        bg ? { backgroundColor: color[bg] } : null,
        radius !== undefined && { borderRadius: tokens.radius[radius] },
        border && { borderWidth: StyleSheet.hairlineWidth, borderColor: color.border },
        flex !== undefined && { flex },
        row && { flexDirection: 'row' },
        center && { alignItems: 'center', justifyContent: 'center' },
        style,
      ]}
      {...rest}
    />
  );
}

export type AppTextProps = TextProps & {
  variant?: 'tiny' | 'caption' | 'subhead' | 'body' | 'title' | 'display';
  tone?: 'text' | 'muted' | 'accent' | 'danger' | 'success';
  weight?: '400' | '500' | '600' | '700';
};

export function AppText({
  variant = 'body',
  tone = 'text',
  weight = '400',
  style,
  ...rest
}: AppTextProps) {
  const { color } = useTheme();
  return (
    <Text
      style={[
        {
          color: color[tone],
          fontSize: tokens.font[variant],
          // The SF styles are a pairing: a 17 pt body has 22 pt of leading, and
          // setting one without the other is how text looks "off" on iOS.
          lineHeight: tokens.lineHeight[variant],
          fontWeight: weight,
          fontFamily: Platform.select({ ios: 'System', default: 'sans-serif' }),
        },
        style,
      ]}
      {...rest}
    />
  );
}
