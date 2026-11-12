import type { ReactNode } from 'react';
import { StyleSheet, View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText } from './Box';

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

      <View
        style={{
          marginHorizontal: tokens.space.lg,
          borderRadius: tokens.radius.lg,
          backgroundColor: color.surface,
          overflow: 'hidden',
        }}
      >
        {rows.map((row, index) => (
          <View key={index}>
            {index > 0 ? (
              <View
                style={{
                  height: StyleSheet.hairlineWidth,
                  backgroundColor: color.border,
                  // iOS insets the separator to the text, not the card edge.
                  marginLeft: tokens.space.lg,
                }}
              />
            ) : null}
            {row}
          </View>
        ))}
      </View>

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
