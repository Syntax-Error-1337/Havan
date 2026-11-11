import { View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { heights, type Bucket } from '../../entities/device/model/history';

/**
 * A bar chart in Views.
 *
 * No chart library: this draws sixty rounded rectangles, and a dependency that
 * renders SVG would be a much larger thing to keep alive than the twenty lines
 * it replaces. If the app ever needs axes, scatter plots or zoom, that is the
 * moment to reach for one.
 */
export function Sparkline({
  buckets,
  isBoolean = false,
  height = 72,
}: {
  buckets: Bucket[];
  isBoolean?: boolean;
  height?: number;
}) {
  const { color } = useTheme();
  const bars = heights(buckets, isBoolean);

  return (
    <View
      style={{
        flexDirection: 'row',
        alignItems: 'flex-end',
        gap: 2,
        height,
        backgroundColor: color.surface,
        borderRadius: tokens.radius.md,
        padding: tokens.space.sm,
      }}
    >
      {bars.map((bar, index) => (
        <View
          key={index}
          style={{
            flex: 1,
            // A gap (null) draws as nothing at all: an offline device is not a
            // device reading zero.
            height: bar === null ? 0 : Math.max(2, bar * (height - 2 * tokens.space.sm)),
            backgroundColor: bar === null ? 'transparent' : color.accent,
            borderRadius: 2,
            opacity: 0.9,
          }}
        />
      ))}
    </View>
  );
}
