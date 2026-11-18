import type { ReactNode } from 'react';
import { View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { Card } from './Card';
import { DeviceArtField } from './DeviceArtField';
import { AppText } from './Box';
import type { IconName } from './Icon';

/**
 * A screen with nothing on it yet.
 *
 * Every empty state in the app had been written by hand, and they had drifted: a bare sentence, a bordered box, a
 * black rectangle with a heading — three answers to the same moment, and none of them wearing the card system the
 * dashboard uses. The moment a customer sees nothing is the moment the product has to look most deliberate.
 *
 * The shape is the same as a device tile, on purpose: a **field** in the accent's own light, the glyph big and cropped
 * by its frame, the words under it, one action. An empty home, an empty rule list and an empty scene list should feel
 * like the same product as a full one.
 */
export function EmptyState({
  accent,
  glyph,
  title,
  body,
  action,
  testID,
}: {
  accent: string;
  glyph: IconName;
  title: string;
  body: string;
  /** The one thing to do next. An empty state that ends without an action ends with a guess. */
  action?: ReactNode;
  testID?: string;
}) {
  const { color } = useTheme();

  return (
    <Card accent={accent} padded={false} testID={testID}>
      <View style={{ padding: 6 }}>
        <DeviceArtField accent={accent} glyph={glyph} emphasis="subject" height={96} />
      </View>
      <View style={{ paddingHorizontal: tokens.space.lg, paddingBottom: tokens.space.lg, gap: tokens.space.sm }}>
        <AppText variant="title" weight="600">
          {title}
        </AppText>
        <AppText variant="caption" tone="muted">
          {body}
        </AppText>
        {action ? <View style={{ paddingTop: tokens.space.xs, alignItems: 'flex-start' }}>{action}</View> : null}
      </View>
      {/* Keeps a focus ring and a pressed state inside the card's own radius rather than the field's. */}
      <View pointerEvents="none" style={{ position: 'absolute', inset: 0, borderRadius: tokens.radius.card, borderWidth: 0.5, borderColor: color.cardEdge }} />
    </Card>
  );
}
