import type { ReactNode } from 'react';
import { StyleSheet, View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText } from './Box';
import { Card } from './Card';

/**
 * An inset grouped section — the shape iOS uses for a list of related things.
 *
 * One rounded card, hairline separators between rows, a 13 pt secondary header
 * above it. Two reasons this matters beyond looking native: the card edge tells a
 * person where the group ends without a box around every row, and the header is
 * the only place a screen can say what the group *is*.
 */
export function Section({
  title,
  footnote,
  children,
}: {
  title?: string;
  /** Explanatory line under the card, as iOS does for anything non-obvious. */
  footnote?: string;
  children: ReactNode;
}) {
  const { color } = useTheme();
  const rows = Array.isArray(children) ? children : [children];

  return (
    <View style={{ marginBottom: tokens.space.xl }}>
      {title ? (
        <AppText
          variant="caption"
          tone="muted"
          style={{
            paddingHorizontal: tokens.space.lg,
            paddingBottom: tokens.space.sm,
            textTransform: 'uppercase',
            letterSpacing: 0.4,
          }}
        >
          {title}
        </AppText>
      ) : null}

      {/* A section is a card, in the same system as the dashboard: radius 22, a lit top edge and one shadow. It used
          to be a flat `surface` rectangle with a 12-point radius, which is why Settings looked like a different app. */}
      <Card padded={false} style={{ marginHorizontal: tokens.space.lg }}>
        <View style={{ overflow: 'hidden' }}>
        {rows.map((row, index) => (
          <View key={index}>
            {index > 0 ? (
              <View
                style={{
                  height: StyleSheet.hairlineWidth,
                  backgroundColor: color.border,
                  // iOS insets a list separator to the text and runs it flush to the trailing edge — but that rule is
                  // for a full-bleed table. A section is now a floating card with a 22-point radius, and a rule that
                  // touches one edge and not the other reads as a table caught inside a rounded box. Inset both.
                  marginHorizontal: tokens.space.lg,
                }}
              />
            ) : null}
            {row}
          </View>
        ))}
        </View>
      </Card>

      {footnote ? (
        <AppText
          variant="caption"
          tone="muted"
          style={{ paddingHorizontal: tokens.space.lg, paddingTop: tokens.space.sm }}
        >
          {footnote}
        </AppText>
      ) : null}
    </View>
  );
}
