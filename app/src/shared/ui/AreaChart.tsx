import { View } from 'react-native';
import { StyleSheet, Text } from 'react-native';
import Svg, { Circle, Defs, LinearGradient, Mask, Path, Rect, Stop, Text as SvgText } from 'react-native-svg';

import type { Bucket } from '../../entities/device/model/history';
import { tokens } from '../config/tokens';
import { buildSegments } from './areaPath';

/**
 * A line with its own shadow under it.
 *
 * The reference screens all show a reading as a shape rather than as a number alone, and a filled
 * area is what makes "it was 19 most of the night and jumped at seven" readable in a glance. Bars
 * answer a different question — how many events per bucket — which is why the device screen keeps
 * the bar sparkline and this card uses the area.
 *
 * Gaps are honoured: a bucket with no reading breaks the line instead of being drawn as zero.
 *
 * ## The current reading, when it is not in the series
 *
 * A home screen shows the freshest value the platform has, and the history behind it can be hours old —
 * a sensor that reports every fifteen minutes leaves a 24-hour chart whose last bucket is 03:36 while the
 * number above it is from 21:30. Two wrong answers are available there: append the live value as another
 * point (a vertical spike at the edge, because it shares its x with the last bucket) or overwrite the last
 * bucket (a bucket from 03:36 drawn as if it held a 21:30 reading). Both were tried on this card and both
 * looked like a rendering bug, because both were one.
 *
 * So the live value is drawn as what it is: a marker of its own, past a gutter, disconnected from the
 * line. `now` says "this is a separate observation, after a gap" instead of connecting two readings that
 * never touched.
 */
export function AreaChart({
  buckets,
  accent,
  height = 96,
  showFill = true,
  now = null,
  nowAt,
  gapColor,
  scale,
  axis,
}: {
  buckets: Bucket[];
  /**
   * The two numbers the line runs between, drawn at the chart's edges.
   *
   * Without them a chart has no scale, and a last reading that happens to be the window's coldest reads as
   * "it crashed" — which is exactly what a reviewer said about a 16.6–17.7 °C day. The numbers are passed in
   * rather than computed here so the unit is formatted once, by the screen that knows the device.
   */
  scale?: { min: string; max: string; color: string } | null;
  /**
   * The two ends of the time axis. A chart with no timeline says "something happened" and nothing else — the
   * reviewer's complaint was exactly that: a shape with no idea whether it covered an hour or a month.
   */
  axis?: { from: string; to: string; color: string } | null;
  accent: string;
  /**
   * When the live reading was taken. The marker is placed at *that* moment, not at "now": a reading from
   * eleven hours ago drawn at the right edge is a claim the house is reporting, which is the one thing a
   * chart must not invent. Absent means the reading is current.
   */
  nowAt?: number;
  /** The colour of the "no readings here" connector. Muted by the caller: accent means measured. */
  gapColor?: string;
  height?: number;
  showFill?: boolean;
  /** The live reading, when it is not already the last bucket. Drawn detached, at the right edge. */
  now?: number | null;
}) {
  /**
   * The x axis is **time**, not index.
   *
   * An index axis draws a twelve-hour gap between the last reading and now as fourteen pixels, which is how
   * a card ended up saying "10 PM–now" above a line that visibly stopped at 11 AM: the picture claimed the
   * readings continued. Placing each bucket at its own timestamp makes a gap look like the time it took,
   * and the axis labels then mean what they say.
   */
  const at = (bucket: Bucket): number => new Date(bucket.t).getTime();
  const width = 320;
  const values = buckets.map((b) => b.v).filter((v) => Number.isFinite(v));
  if (buckets.length < 2 || values.length < 2) {
    return <View style={{ height }} />;
  }

  // The live value is part of the scale, or its marker floats outside the chart it belongs to.
  const scaleValues = now === null ? values : [...values, now];
  const min = Math.min(...scaleValues);
  const max = Math.max(...scaleValues);
  const span = max - min || 1;
  const pad = 6;

  // A gutter keeps the line's last point from colliding with the live marker at the edge.
  const gutter = now === null ? 0 : 14;
  const plot = width - gutter;

  const first = at(buckets[0]!);
  const lastRecorded = at(buckets[buckets.length - 1]!);
  // The axis is the requested window, ending at *now* — that is what makes a gap visible instead of pushing
  // the last reading against the right edge and calling it current. The marker is placed on that axis at its
  // own moment, so an eleven-hour-old reading sits eleven hours back, with the empty space doing the talking.
  const nowMoment = nowAt ?? Date.now();
  const end = Math.max(lastRecorded, Date.now());
  const spanMs = Math.max(end - first, 1);
  const x = (i: number) => ((at(buckets[i]!) - first) / spanMs) * plot;
  const nowX = now === null ? null : Math.max(0, Math.min(plot, ((nowMoment - first) / spanMs) * plot));
  const y = (v: number) => pad + (1 - (v - min) / span) * (height - pad * 2);

  // Gaps break the line, and the fill closes under each stretch's own last reading. `buildSegments` is pure
  // because the previous inline version drew `L NaN,NaN` for every missing bucket while the comment above it
  // claimed the opposite.
  const { line, area, lastIndex, runs } = buildSegments(buckets, x, y, height);
  // The tower of defensive fallbacks is gone: `buildSegments` guarantees `lastIndex` is the last bucket that
  // held a reading, and `values.length >= 2` guarantees there is at least one.
  const lastX = x(Math.max(lastIndex, 0));

  return (
    <View style={{ height }}>
      <Svg width="100%" height={height} viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none">
        <Defs>
          {/* Vertical: brightest under the line, gone at the baseline. */}
          <LinearGradient id="area" x1="0" y1="0" x2="0" y2="1">
            <Stop offset="0" stopColor={accent} stopOpacity="0.45" />
            <Stop offset="1" stopColor={accent} stopOpacity="0" />
          </LinearGradient>
          {/* Horizontal: the fill dissolves into the gap on the right instead of ending in a vertical wall.
              That wall was being read as a reading that crashed to the floor — the last bucket's value is
              simply the last one there was. */}
          {/* A mask paints by alpha only, so its colour is a formality — but it is still a literal, and the
              token gate exists so that a screen cannot quietly decide on a colour the operator owns. */}
          <LinearGradient id="areaFade" x1="0" y1="0" x2="1" y2="0">
            <Stop offset="0" stopColor={tokens.chart.mask} stopOpacity="1" />
            <Stop offset="0.72" stopColor={tokens.chart.mask} stopOpacity="1" />
            <Stop offset="1" stopColor={tokens.chart.mask} stopOpacity="0" />
          </LinearGradient>
          <Mask id="areaMask">
            <Rect x="0" y="0" width={width} height={height} fill="url(#areaFade)" />
          </Mask>
        </Defs>
        {showFill ? <Path d={area} fill="url(#area)" mask="url(#areaMask)" /> : null}
        {scale ? (
          <>
            {/* At the top and bottom of the *plotted* band, not at the axis ends: a truncated axis that does
                not say where it was truncated is a chart nobody can argue with, including the person it is
                about. */}
            <SvgText x={width - 2} y={12} fontSize="10" fill={scale.color} textAnchor="end">
              {scale.max}
            </SvgText>
            <SvgText x={width - 2} y={height - 4} fontSize="10" fill={scale.color} textAnchor="end">
              {scale.min}
            </SvgText>
          </>
        ) : null}
        <Path d={line} stroke={accent} strokeWidth="2.4" fill="none" strokeLinejoin="round" strokeLinecap="round" />
        {now === null ? (
          /* The last *reading* is the current value: mark it where its own stretch ends. Marking the last
             bucket instead put the dot at the right edge whenever the device had gone quiet, which is how a
             sensor that stopped nine hours ago grew a spike it never reported. */
          <>
            <Circle cx={lastX} cy={y(values[values.length - 1]!)} r="4" fill={accent} />
            <Circle cx={lastX} cy={y(values[values.length - 1]!)} r="8" fill={accent} fillOpacity="0.22" />
          </>
        ) : (
          /* A reading taken after a gap. The marker is its own point, joined to the line by a dotted
             segment: solid would claim those hours were measured, a floating dot with no line at all
             reads as a rendering mistake. Dotted is the honest middle — "no readings in between". */
          <>
            {nowX !== null && nowX - lastX > 6 ? (
              <Path
                d={`M${lastX.toFixed(1)},${y(values[values.length - 1]!).toFixed(1)} L${nowX.toFixed(1)},${y(now).toFixed(1)}`}
                stroke={gapColor ?? accent}
                strokeWidth="1.6"
                strokeDasharray="2 3"
                fill="none"
                strokeLinecap="round"
              />
            ) : null}
            {/* There was a dotted drop from the marker to the axis here, meant to say "the last reading was
                here". Two reviewers, shown the same card, read it as a temperature that had plunged to the
                floor — a vertical line at the end of a chart is a cliff, whatever it is labelled. The marker
                and the dotted connector to the line carry the same fact without drawing a fall. */}
            <Circle cx={nowX ?? plot} cy={y(now)} r="4" fill={accent} />
            <Circle
              cx={nowX ?? plot}
              cy={y(now)}
              r="8.5"
              fill="none"
              stroke={accent}
              strokeWidth="1.4"
              strokeOpacity="0.55"
            />
          </>
        )}
      </Svg>
      {axis ? (
        <View style={{ flexDirection: 'row', justifyContent: 'space-between', marginTop: 2 }}>
          <Text style={[styles.axis, { color: axis.color }]}>{axis.from}</Text>
          <Text style={[styles.axis, { color: axis.color }]}>{axis.to}</Text>
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  axis: { fontSize: 10, fontVariant: ['tabular-nums'] },
});
