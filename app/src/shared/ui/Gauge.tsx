import { useEffect, useRef } from 'react';
import { Animated, Easing, View } from 'react-native';
import Svg, { Circle, G } from 'react-native-svg';

import { useTheme, tokens } from '../../app-shell/theme';
import { withAlpha } from '../config/tokens';
import { AppText } from './Box';

const AnimatedCircle = Animated.createAnimatedComponent(Circle);

/**
 * A ring gauge: where a value sits inside the range it can take.
 *
 * The dashboard could say `40 %` and it did. What it could not say is *where in the range* that is — the one thing a
 * person adjusting a dimmer or a thermostat is actually asking. A shape answers it before the number is read, which
 * is why the reference dashboards lead with one.
 *
 * Drawn as a 270° arc with a gap at the bottom, the way a dial is drawn on a physical controller: the gap is the
 * stroke's own "you can turn this", and it keeps the ends off the centre of the value underneath.
 *
 * The sweep animates from the previous value over `tokens.motion.gauge`, because a ring that jumps reads as a new
 * control rather than as the same one answering.
 */
export function Gauge({
  value,
  min,
  max,
  accent,
  size = 132,
  caption,
  showEnds = true,
  children,
}: {
  value: number;
  min: number;
  max: number;
  accent: string;
  size?: number;
  /** A word under the number, inside the ring: "target", "brightness". */
  caption?: string;
  /**
   * The two ends of the scale.
   *
   * On by default, because `1 … 100` answers "where in its range" for a value with a unit nobody knows by heart. Off
   * for a percentage: `80 %` already says 80 of 100, and a reviewer read the ends there as noise — correctly.
   */
  showEnds?: boolean;
  /** The reading and its unit, drawn at the centre. */
  children?: React.ReactNode;
}) {
  const { color } = useTheme();

  const span = Math.max(max - min, Number.EPSILON);
  const pct = Math.min(1, Math.max(0, (value - min) / span));

  const stroke = Math.max(8, Math.round(size * 0.075));
  const radius = (size - stroke) / 2;
  const circumference = 2 * Math.PI * radius;
  const sweep = 0.75; // 270°
  const arc = circumference * sweep;

  const progress = useRef(new Animated.Value(pct)).current;

  useEffect(() => {
    Animated.timing(progress, {
      toValue: pct,
      duration: tokens.motion.gauge,
      easing: Easing.out(Easing.cubic),
      // `useNativeDriver` cannot drive an SVG stroke; the frame cost of one ring is small and measured.
      useNativeDriver: false,
    }).start();
  }, [pct, progress]);

  const dashOffset = progress.interpolate({
    inputRange: [0, 1],
    outputRange: [arc, 0],
  });

  // Where the arc ends, so a knob can sit on it — in the knob's own coordinates, which are inside the `<G>` that
  // rotates everything by 135°. Getting this wrong put the thumb at the *start* of an 80 % arc, on the far side of the
  // ring from the fill's end, which a reviewer read as "a decorative dot" — correctly.
  const endAngle = 270 * pct * (Math.PI / 180);
  const knobX = size / 2 + radius * Math.cos(endAngle);
  const knobY = size / 2 + radius * Math.sin(endAngle);

  return (
    <View style={{ width: size, height: size, alignItems: 'center', justifyContent: 'center' }}>
      <Svg width={size} height={size} style={{ position: 'absolute' }}>
        {/* Rotated so the gap sits at the bottom and the arc starts where a dial starts: lower left. */}
        <G rotation={135} origin={`${size / 2}, ${size / 2}`}>
          <Circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            stroke={color.track}
            strokeWidth={stroke}
            strokeLinecap="round"
            fill="none"
            strokeDasharray={`${arc} ${circumference}`}
          />
          <AnimatedCircle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            stroke={accent}
            strokeWidth={stroke}
            strokeLinecap="round"
            fill="none"
            strokeDasharray={`${arc} ${circumference}`}
            strokeDashoffset={dashOffset}
          />
          {pct > 0.02 ? (
            <Circle cx={knobX} cy={knobY} r={stroke * 0.62} fill={accent} stroke="none" />
          ) : null}
        </G>
      </Svg>

      <View style={{ alignItems: 'center', paddingHorizontal: stroke * 2 }}>
        {children}
        {caption ? (
          <AppText
            variant="tiny"
            tone="muted"
            numberOfLines={1}
            style={{ marginTop: 2, letterSpacing: 0.4 }}
          >
            {caption}
          </AppText>
        ) : null}
      </View>

      {/* The ends of the scale, under the gap. Without them the ring says "somewhere" rather than "where". */}
      {showEnds ? <View
        style={{
          position: 'absolute',
          bottom: 2,
          left: 0,
          right: 0,
          flexDirection: 'row',
          justifyContent: 'space-between',
          paddingHorizontal: size * 0.06,
        }}
      >
        <AppText variant="tiny" tone="muted" style={{ opacity: 0.85 }}>
          {String(Math.round(min))}
        </AppText>
        <AppText variant="tiny" tone="muted" style={{ opacity: 0.85 }}>
          {String(Math.round(max))}
        </AppText>
      </View> : null}
    </View>
  );
}

/** The ring's track, exported so a screen can draw the same circle behind custom centre content. */
export const gaugeTrack = (accent: string) => withAlpha(accent, 0.2);
