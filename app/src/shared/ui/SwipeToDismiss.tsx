import { useRef, useState, type ReactNode } from 'react';
import { Animated, PanResponder, StyleSheet, View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText } from './Box';
import { useReducedMotion } from '../lib/useReducedMotion';

/** How far the row must travel before the gesture counts. Short enough to feel like a flick, long enough that a
 *  diagonal scroll is not mistaken for one. */
export const DISMISS_AT = 88;

/**
 * Whether a drag is a dismissal. Pure, so it can be tested without a pan gesture: the distance is the only input that
 * decides, and a threshold nobody can test is a threshold that drifts.
 */
export function shouldDismiss(dx: number): boolean {
  return dx <= -DISMISS_AT;
}

/** A drag is a dismissal only if it travels mostly sideways — the same rule the responder claims with. */
export function isHorizontalSwipe(dx: number, dy: number): boolean {
  return Math.abs(dx) > 12 && Math.abs(dx) > Math.abs(dy) * 1.4;
}

/**
 * Swipe a row to the left to throw it away.
 *
 * The alert feed had no way to remove anything, and "mark all read" was the only control. A trailing Delete button on
 * every row would put the destructive action of the whole screen at the thumb's resting place; a swipe is the gesture
 * iOS customers already make on mail and messages, and it leaves the list quiet.
 *
 * Two things this wrapper refuses to do:
 *
 * - **Hide the affordance in a gesture only.** A screen reader cannot swipe to dismiss, so the row also exposes an
 *   `accessibilityAction` named "dismiss" — the same work, reachable without a gesture.
 * - **Animate when the customer asked for less motion.** With reduced motion the row snaps back or leaves; it does not
 *   spring.
 */
export function SwipeToDismiss({
  onDismiss,
  children,
  label = 'Remove',
}: {
  onDismiss: () => void;
  children: ReactNode;
  label?: string;
}) {
  const { color } = useTheme();
  const reduced = useReducedMotion();
  const x = useRef(new Animated.Value(0)).current;
  // The callback in a ref: `PanResponder.create` runs once, so an inline `onDismiss` would be frozen at the first
  // render and every later row would dismiss whichever alert mounted first.
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;
  // The word sits behind the row, but only while the row is actually moving: rendered always it is a stray "Dismiss"
  // in the middle of the feed for anyone reading with a screen reader, and a word you can see through a card for
  // everyone else.
  const [pulling, setPulling] = useState(false);

  const settle = (to: number) =>
    reduced
      ? x.setValue(to)
      : Animated.spring(x, { toValue: to, useNativeDriver: true, bounciness: 0, speed: 18 }).start();

  const responder = useRef(
    PanResponder.create({
      // Horizontal only, and only when the drag is more horizontal than vertical — otherwise every scroll on the feed
      // becomes a dismiss.
      onMoveShouldSetPanResponder: (_e, g) => isHorizontalSwipe(g.dx, g.dy),
      onPanResponderGrant: () => undefined,
      onPanResponderMove: (_e, g) => {
        // Left only: a rightward drag has no meaning here, and letting one through would make the row look loose.
        x.setValue(Math.min(0, Math.max(-140, g.dx)));
        setPulling(g.dx < -4);
      },
      onPanResponderRelease: (_e, g) => {
        setPulling(false);
        if (shouldDismiss(g.dx)) {
          settle(-400);
          dismiss.current();
        } else {
          settle(0);
        }
      },
      onPanResponderTerminate: () => {
        setPulling(false);
        settle(0);
      },
    }),
  ).current;

  return (
    <View
      // The action behind the row, visible only while it is being dragged away.
      style={{ justifyContent: 'center' }}
      accessibilityActions={[{ name: 'dismiss', label }]}
      onAccessibilityAction={(event) => {
        if (event.nativeEvent.actionName === 'dismiss') dismiss.current();
      }}
    >
      {pulling ? (
        <View
          pointerEvents="none"
          style={{
            position: 'absolute',
            right: tokens.space.md,
            top: 0,
            bottom: 0,
            justifyContent: 'center',
          }}
        >
          <AppText variant="caption" weight="600" tone="danger">
            {label}
          </AppText>
        </View>
      ) : null}
      <Animated.View
        style={{
          transform: [{ translateX: x }],
          backgroundColor: color.bg,
          // A hairline under the sliding row, so the "Dismiss" behind it is invisible until the row moves.
          borderBottomWidth: StyleSheet.hairlineWidth,
          borderBottomColor: 'transparent',
        }}
        {...responder.panHandlers}
      >
        {children}
      </Animated.View>
    </View>
  );
}
