import { Platform, type ViewStyle } from 'react-native';

import { tokens, withAlpha } from '../config/tokens';

/**
 * One light source, three heights — written once.
 *
 * iOS, Android and web express the same shadow with three different properties, and a screen that sets them by hand
 * ends up matching on one platform and flat on the other two. `tokens.elevation` holds the numbers; this turns them
 * into the style each platform actually reads.
 *
 * The colour is the palette's `shadow`, so a white-label operator's brand never changes how deep the app feels.
 */
export function elevation(level: 1 | 2 | 3, shadowColour: string): ViewStyle {
  const { y, blur, opacity } = tokens.elevation[level];
  const colour = withAlpha(shadowColour, opacity);

  if (Platform.OS === 'web') {
    // `boxShadow` is the one property the web build understands; the RN shadow props are ignored there.
    return { boxShadow: `0 ${y}px ${blur}px ${colour}` } as ViewStyle;
  }

  return Platform.select({
    android: { elevation: level === 1 ? 2 : level === 2 ? 6 : 10 },
    default: {
      shadowColor: shadowColour,
      shadowOffset: { width: 0, height: y },
      shadowOpacity: opacity,
      shadowRadius: blur / 2,
    },
  }) as ViewStyle;
}
