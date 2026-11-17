import { View } from 'react-native';
import Svg, { Defs, LinearGradient, Path, Stop } from 'react-native-svg';

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
 */
export function AreaChart({
  buckets,
  accent,
  height = 96,
  showFill = true,
}: {
  buckets: Bucket[];
  accent: string;
  height?: number;
  showFill?: boolean;
}) {
  const width = 320;
  const values = buckets.map((b) => b.v).filter((v) => Number.isFinite(v));
  if (buckets.length < 2 || values.length < 2) {
    return <View style={{ height }} />;
  }

  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const pad = 6;

  const x = (i: number) => (i / (buckets.length - 1)) * width;
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
  area += ` L${width},${height} Z`;

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
      </Svg>
    </View>
  );
}
