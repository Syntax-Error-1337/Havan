import { View } from 'react-native';
import Svg, { Circle, Defs, LinearGradient, Path, Stop } from 'react-native-svg';

import type { Bucket } from '../../entities/device/model/history';

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
}: {
  buckets: Bucket[];
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

  let line = '';
  let area = `M0,${height} `;
  let started = false;

  buckets.forEach((bucket, i) => {
    const px = x(i);
    const py = y(bucket.v);
    if (!started) {
      line += `M${px.toFixed(1)},${py.toFixed(1)}`;
      area += `L${px.toFixed(1)},${py.toFixed(1)}`;
      started = true;
    } else {
      line += ` L${px.toFixed(1)},${py.toFixed(1)}`;
      area += ` L${px.toFixed(1)},${py.toFixed(1)}`;
    }
  });
  // The fill closes under the last *reading*, not under the axis end: filling to the edge would paint the
  // gap as if it had been measured.
  const lastX = x(buckets.length - 1);
  area += ` L${lastX.toFixed(1)},${height} Z`;

  return (
    <View style={{ height }}>
      <Svg width="100%" height={height} viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none">
        <Defs>
          <LinearGradient id="area" x1="0" y1="0" x2="0" y2="1">
            <Stop offset="0" stopColor={accent} stopOpacity="0.45" />
            <Stop offset="1" stopColor={accent} stopOpacity="0" />
          </LinearGradient>
        </Defs>
        {showFill ? <Path d={area} fill="url(#area)" /> : null}
        <Path d={line} stroke={accent} strokeWidth="2.4" fill="none" strokeLinejoin="round" strokeLinecap="round" />
        {now === null ? (
          /* The last bucket *is* the current reading: mark it where the line ends. */
          <>
            <Circle cx={x(buckets.length - 1)} cy={y(values[values.length - 1]!)} r="4" fill={accent} />
            <Circle cx={x(buckets.length - 1)} cy={y(values[values.length - 1]!)} r="8" fill={accent} fillOpacity="0.22" />
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
            {/* A dotted drop to the axis, not just a floating dot: it says "the last reading was here" and
                makes the empty stretch to the right read as time that passed without data, rather than as a
                line that gave up halfway. */}
            <Path
              d={`M${(nowX ?? plot).toFixed(1)},${y(now).toFixed(1)} L${(nowX ?? plot).toFixed(1)},${height}`}
              stroke={gapColor ?? accent}
              strokeWidth="1"
              strokeDasharray="1 4"
              fill="none"
              strokeOpacity="0.7"
            />
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
    </View>
  );
}
