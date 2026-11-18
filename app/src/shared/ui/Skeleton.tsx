import { useEffect, useRef } from 'react';
import { AccessibilityInfo, Animated, Easing, Platform, View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';

/**
 * A block in the shape of what is coming.
 *
 * The old loading state was three grey rectangles of equal height, which told the reader "something
 * is happening" and nothing else. A skeleton that matches the real layout — rail, hero, two tiles —
 * tells them what to expect, and the page does not jump when the data lands.
 *
 * The pulse is opt-out: a person who set "reduce motion" gets a still block rather than a breathing
 * one, because a loading state is not worth fighting an accessibility setting over.
 */
export function Skeleton({
  height,
  width = '100%',
  radius = tokens.radius.lg,
}: {
  height: number;
  width?: number | `${number}%`;
  radius?: number;
}) {
  const { color } = useTheme();
  const pulse = useRef(new Animated.Value(0.55)).current;

  useEffect(() => {
    let cancelled = false;
    let animation: Animated.CompositeAnimation | null = null;

    AccessibilityInfo.isReduceMotionEnabled()
      .then((reduced) => {
        if (cancelled || reduced) return;
        animation = Animated.loop(
          Animated.sequence([
            Animated.timing(pulse, {
              toValue: 0.9,
              duration: 900,
              easing: Easing.inOut(Easing.quad),
              // Web has no native animated module; asking for it only produces a warning on every
              // cold start, which is noise that hides real ones in the log.
              useNativeDriver: Platform.OS !== 'web',
            }),
            Animated.timing(pulse, {
              toValue: 0.55,
              duration: 900,
              easing: Easing.inOut(Easing.quad),
              // Web has no native animated module; asking for it only produces a warning on every
              // cold start, which is noise that hides real ones in the log.
              useNativeDriver: Platform.OS !== 'web',
            }),
          ]),
        );
        animation.start();
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      animation?.stop();
    };
  }, [pulse]);

  return (
    <Animated.View
      style={{
        height,
        width,
        borderRadius: radius,
        backgroundColor: color.glass,
        opacity: pulse,
      }}
    />
  );
}

/** The home screen's shape, before it has any data. */
/**
 * A list that is loading, in the shape of the list.
 *
 * Alerts, automations and homes each showed a single grey word — "Loading…" — in the middle of an empty
 * screen. On a slow connection that is a second and a half of a screen that looks broken, and then the
 * layout jumps. These rows are the height and rhythm of a real card, so the page settles into place instead
 * of rebuilding itself.
 */
export function ListSkeleton({
  rows = 3,
  rowHeight = 84,
  showHeader = true,
}: {
  rows?: number;
  rowHeight?: number;
  /** A page title block above the rows, for a screen that draws its own header. */
  showHeader?: boolean;
}) {
  return (
    <View testID="list-skeleton" style={{ paddingHorizontal: tokens.space.lg, gap: tokens.space.md }}>
      {showHeader ? (
        <View style={{ gap: 6 }}>
          <Skeleton height={34} width="55%" />
          <Skeleton height={13} width="40%" />
        </View>
      ) : null}
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} height={rowHeight} />
      ))}
    </View>
  );
}

export function HomeSkeleton() {
  return (
    <View style={{ paddingHorizontal: tokens.space.lg, gap: tokens.space.md }}>
      <View style={{ flexDirection: 'row', gap: 8 }}>
        <Skeleton height={86} width="31%" />
        <Skeleton height={86} width="31%" />
        <Skeleton height={86} width="31%" />
      </View>
      <Skeleton height={228} />
      <View style={{ flexDirection: 'row', gap: 12 }}>
        <Skeleton height={132} width="47%" />
        <Skeleton height={132} width="47%" />
      </View>
    </View>
  );
}
