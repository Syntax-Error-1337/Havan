import { StyleSheet, View } from 'react-native';
import Svg, { Defs, LinearGradient, RadialGradient, Rect, Stop } from 'react-native-svg';

import { useTheme } from '../../app-shell/theme';

/**
 * The canvas, as light rather than as a colour.
 *
 * A single vertical gradient reads as a coloured rectangle. Three overlapping radial washes over a
 * near-black base read as a room lit from three sides, which is what makes a translucent card above
 * it look like glass instead of like a grey box. It is drawn in SVG rather than as stacked views
 * because a radial gradient has no seam and stacking linear gradients always leaves one.
 *
 * The bloom positions are fixed rather than random: a phone that repaints its background on every
 * render looks broken, and a mood that moves is a mood nobody can recognise twice.
 */
export function MeshBackground() {
  const { dark } = useTheme();

  return (
    <View pointerEvents="none" style={StyleSheet.absoluteFill}>
      <Svg width="100%" height="100%" viewBox="0 0 400 900" preserveAspectRatio="xMidYMid slice">
        <Defs>
          <LinearGradient id="base" x1="0" y1="0" x2="0.4" y2="1">
            <Stop offset="0" stopColor={dark ? '#0A1024' : '#EEF2FB'} />
            <Stop offset="0.55" stopColor={dark ? '#080B18' : '#E7ECF8'} />
            <Stop offset="1" stopColor={dark ? '#04060E' : '#DDE4F3'} />
          </LinearGradient>

          {/* Teal high left: the cool light a room gets from a window. */}
          <RadialGradient id="teal" cx="0.5" cy="0.5" r="0.5">
            <Stop offset="0" stopColor={dark ? '#0E7490' : '#7DD3FC'} stopOpacity={dark ? 0.55 : 0.5} />
            <Stop offset="0.55" stopColor={dark ? '#0E7490' : '#7DD3FC'} stopOpacity={dark ? 0.16 : 0.16} />
            <Stop offset="1" stopColor="#0E7490" stopOpacity="0" />
          </RadialGradient>

          {/* Violet right: the accent family, low and wide so it never bands. */}
          <RadialGradient id="violet" cx="0.5" cy="0.5" r="0.5">
            <Stop offset="0" stopColor={dark ? '#6D28D9' : '#A78BFA'} stopOpacity={dark ? 0.5 : 0.42} />
            <Stop offset="0.6" stopColor={dark ? '#6D28D9' : '#A78BFA'} stopOpacity={dark ? 0.14 : 0.12} />
            <Stop offset="1" stopColor="#6D28D9" stopOpacity="0" />
          </RadialGradient>

          {/* Indigo at the foot: keeps the tab bar from sitting on pure black. */}
          <RadialGradient id="indigo" cx="0.5" cy="0.5" r="0.5">
            <Stop offset="0" stopColor={dark ? '#1E3A8A' : '#93C5FD'} stopOpacity={dark ? 0.45 : 0.3} />
            <Stop offset="1" stopColor="#1E3A8A" stopOpacity="0" />
          </RadialGradient>
        </Defs>

        <Rect x="0" y="0" width="400" height="900" fill="url(#base)" />
        <Rect x="-160" y="-180" width="620" height="620" fill="url(#teal)" />
        <Rect x="120" y="130" width="700" height="700" fill="url(#violet)" />
        <Rect x="-180" y="520" width="760" height="760" fill="url(#indigo)" />
      </Svg>
    </View>
  );
}
