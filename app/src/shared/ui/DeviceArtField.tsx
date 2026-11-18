import { useId, type ReactNode } from 'react';
import { StyleSheet, View, type ViewStyle } from 'react-native';
import Svg, { Defs, RadialGradient, Rect, Stop } from 'react-native-svg';

import { useTheme, tokens } from '../../app-shell/theme';
import { withAlpha } from '../config/tokens';
import { Icon, type IconName } from './Icon';

/**
 * The field a device stands in.
 *
 * This is the honest stand-in for product photography. A white-label platform cannot ship a photograph of hardware
 * it has never seen — the same vendor's bulb, a thermostat from another, a sensor from a third — and stock imagery
 * would be a lie about the thing in the customer's hand. What it *can* do is give every device the light and colour
 * of a product shot: a wash in the device's own category hue, one soft bloom, and its glyph set large and half
 * off the edge, the way a product sits in a studio frame.
 *
 * The result is the thing the grid was missing: eight devices read as eight *things* rather than eight cards.
 *
 * Restraint is the design. One wash, one bloom, one glyph — not a mesh, not two gradients, not a pattern. Everything
 * in here is decorative: it is hidden from screen readers, because the name and the reading are already announced.
 */
export function DeviceArtField({
  accent,
  glyph,
  height = 84,
  dimmed = false,
  style,
  children,
}: {
  accent: string;
  glyph: IconName;
  height?: number;
  /**
   * The device is not reporting.
   *
   * A field at full strength says "this is happening now", and on a home where six of six devices were offline every
   * card still looked live — the sentence said offline, the surface disagreed. A dimmed field keeps its identity and
   * gives up its presence.
   */
  dimmed?: boolean;
  style?: ViewStyle;
  /** The reading, a control, a chip — whatever sits *on* the field. */
  children?: ReactNode;
}) {
  const { dark } = useTheme();
  // Gradient ids are global in SVG. Two cards with the same id is how one card's bloom paints another's.
  const id = `field-${useId().replace(/[^a-zA-Z0-9]/g, '')}`;

  // Light mode needs less of the hue to stay a surface; dark mode needs more, because 6 % white on near-black is
  // invisible and the field is what tells a bulb from a sensor at a glance.
  const presence = dimmed ? 0.5 : 1;
  const wash = withAlpha(accent, (dark ? 0.22 : 0.1) * presence);
  const bloomTop = (dark ? 0.42 : 0.3) * presence;

  return (
    <View
      style={[
        {
          height,
          borderRadius: tokens.radius.field,
          backgroundColor: wash,
          overflow: 'hidden',
        },
        style,
      ]}
    >
      <Svg style={StyleSheet.absoluteFill} width="100%" height="100%">
        <Defs>
          <RadialGradient id={id} cx="78%" cy="12%" rx="72%" ry="110%">
            <Stop offset="0" stopColor={accent} stopOpacity={bloomTop} />
            <Stop offset="0.55" stopColor={accent} stopOpacity={bloomTop * 0.45} />
            <Stop offset="1" stopColor={accent} stopOpacity={0} />
          </RadialGradient>
        </Defs>
        <Rect x="0" y="0" width="100%" height="100%" fill={`url(#${id})`} />
      </Svg>

      {/* The glyph, oversized and cropped by the field's own edge: a product sits in a frame, it does not float in
          the middle of one. Anchored so its centre lands on the field's corner. */}
      <View
        pointerEvents="none"
        accessibilityElementsHidden
        importantForAccessibility="no-hide-descendants"
        style={{
          position: 'absolute',
          right: -height * 0.1,
          bottom: -height * 0.14,
          opacity: dark ? 0.16 : 0.12,
        }}
      >
        {/* 0.8 of the field, not 1.05. At full height the glyph was the loudest thing on the card and it sat
            behind the reading — two focal points competing, and the wrong one winning. The glyph says what kind of
            device this is; the number says what it is doing, and the number wins. */}
        <Icon name={glyph} size={height * 0.8} color={accent} />
      </View>

      {children}
    </View>
  );
}
