import type { ReactNode } from 'react';
import { Pressable, View, type ViewStyle } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { withAlpha } from '../config/tokens';
import { elevation } from './elevation';

/**
 * A content surface.
 *
 * Not glass — and that is the whole point of this component existing. Apple's rule is blunt: "Don't use Liquid Glass
 * in the content layer … it can result in unnecessary complexity and a confusing visual hierarchy"
 * (`materials.md › Liquid Glass`), and the review checklist in the design skill calls glass on cards "a defect". This
 * app had it on every card, which is why the dashboard read as a set of soft grey rectangles: when everything floats,
 * nothing does.
 *
 * A card is a *material* instead: a near-opaque surface, a 1 pt highlight along its top edge so it has a light
 * source, a hairline border, and one of three shadows. Depth comes from the light, not from the blur.
 *
 * Glass stays where it belongs: the tab bar, the sheets, and the floating room capsule — the functional layer, over
 * content that actually scrolls beneath it.
 */
export function Card({
  children,
  onPress,
  accent,
  level = 1,
  padded = true,
  style,
  accessibilityLabel,
  accessibilityHint,
  disabled = false,
  containsControls = false,
  testID,
}: {
  children: ReactNode;
  onPress?: () => void;
  /** Category accent: tints the border so a card still belongs to its device, not to the layout. */
  accent?: string;
  /** 1 resting, 2 the hero, 3 the floating capsule. */
  level?: 1 | 2 | 3;
  padded?: boolean;
  style?: ViewStyle;
  accessibilityLabel?: string;
  accessibilityHint?: string;
  disabled?: boolean;
  /**
   * The card holds its own controls — a switch, a stepper.
   *
   * `react-native-web` renders a Pressable with `accessibilityRole="button"` as a real `<button>`, and a button
   * cannot contain a button: the browser warns, the screen reader loses the inner control, and a click on the inner
   * one can be swallowed by the outer. So a card with controls inside it is a **surface**, and the pressable part is
   * the caller's job — a header region that fills the card, with the controls as its siblings rather than its
   * children. Found by pressing the hero's new stepper: the DOM had a `<button>` wrapping two more.
   */
  containsControls?: boolean;
  testID?: string;
}) {
  const { color } = useTheme();

  const surface: ViewStyle = {
    backgroundColor: color.card,
    borderRadius: tokens.radius.card,
    borderWidth: 0.5,
    borderColor: accent ? withAlpha(accent, 0.26) : color.border,
    overflow: 'hidden',
    ...elevation(level, color.shadow),
  };

  const body = (
    <>
      {/* The light source. A surface with a lit top edge reads as a physical object; the same surface without one
          reads as a rounded rectangle. Inset from the corners so it follows the radius instead of cutting it. */}
      <View
        pointerEvents="none"
        style={{
          position: 'absolute',
          top: 0,
          left: tokens.radius.card / 2,
          right: tokens.radius.card / 2,
          height: 1,
          backgroundColor: color.cardEdge,
        }}
      />
      <View style={{ padding: padded ? tokens.space.lg : 0 }}>{children}</View>
    </>
  );

  if (!onPress || containsControls) {
    return (
      <View style={[surface, style]} testID={testID}>
        {body}
      </View>
    );
  }

  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel}
      accessibilityHint={accessibilityHint}
      testID={testID}
      style={({ pressed }) => [surface, style, pressed ? { transform: [{ scale: 0.97 }] } : null]}
    >
      {body}
    </Pressable>
  );
}
