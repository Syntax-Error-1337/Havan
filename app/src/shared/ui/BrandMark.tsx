import { View } from 'react-native';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText } from './Box';

/**
 * The operator's mark, as the app draws it.
 *
 * Sign-in had one and the first-run screen — the *other* thing a new customer sees — had none, so the moment after
 * "Create account" looked like a different product: no name, no mark, just a question. The mark is the same
 * component in both places now, so a white-label operator's name and initials appear wherever the app introduces
 * itself.
 */
export function BrandMark({ size = 44 }: { size?: number }) {
  const { color, brand } = useTheme();

  return (
    <View style={{ flexDirection: 'row', alignItems: 'center', gap: tokens.space.sm }}>
      <View
        style={{
          width: size,
          height: size,
          borderRadius: size / 2,
          alignItems: 'center',
          justifyContent: 'center',
          backgroundColor: color.accent,
        }}
      >
        <AppText
          variant="caption"
          weight="700"
          style={{ color: color.onAccent, fontSize: Math.round(size * 0.32) }}
        >
          {brand.logoText}
        </AppText>
      </View>
      <AppText variant="display" weight="700">
        {brand.productName}
      </AppText>
    </View>
  );
}
