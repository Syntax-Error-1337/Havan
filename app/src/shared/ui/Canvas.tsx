import { LinearGradient } from 'expo-linear-gradient';
import type { ReactNode } from 'react';
import { StyleSheet, View, type ViewStyle } from 'react-native';

import { useTheme } from '../../app-shell/theme';

/**
 * The canvas.
 *
 * A flat black rectangle makes a translucent card look like a grey box, so depth has to come from
 * somewhere: the gradient is two stops of the same hue family, drawn behind everything, and every
 * surface above it is semi-transparent. That is how the card reads as glass without a blur pass,
 * which would cost a frame on every scroll on the devices this app runs on.
 */
export function Canvas({ children, style }: { children: ReactNode; style?: ViewStyle }) {
  const { color } = useTheme();
  return (
    <View style={[{ flex: 1 }, style]}>
      <LinearGradient
        colors={[color.bgTop, color.bgBottom]}
        start={{ x: 0.1, y: 0 }}
        end={{ x: 0.9, y: 1 }}
        style={StyleSheet.absoluteFill}
      />
      {children}
    </View>
  );
}
