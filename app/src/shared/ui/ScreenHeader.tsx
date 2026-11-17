import { Pressable, View } from 'react-native';
import { useRouter } from 'expo-router';

import { useTheme, tokens } from '../../app-shell/theme';
import { AppText } from './Box';
import { Icon } from './Icon';

/**
 * The header of a screen you pushed to: a way back, and where you are.
 *
 * Two screens had grown their own copy of this — a rotated chevron, a hit slop, a title — and the copies
 * were already starting to disagree. One component means "back" behaves the same everywhere, including
 * when the screen was opened by a deep link and there is nothing behind it.
 */
export function ScreenHeader({
  title,
  onBack,
  right,
}: {
  title: string;
  /** Defaults to "go back, or go home if this is the first screen". */
  onBack?: () => void;
  right?: React.ReactNode;
}) {
  const router = useRouter();
  const { color } = useTheme();

  const leave = () => {
    if (onBack) return onBack();
    if (router.canGoBack()) router.back();
    else router.replace('/');
  };

  return (
    <View
      style={{
        flexDirection: 'row',
        alignItems: 'center',
        gap: tokens.space.sm,
        paddingHorizontal: tokens.space.lg,
        paddingVertical: tokens.space.md,
      }}
    >
      <Pressable accessibilityRole="button" accessibilityLabel="Back" hitSlop={12} onPress={leave}>
        <View style={{ transform: [{ rotate: '180deg' }] }}>
          <Icon name="chevron-right" size={20} color={color.muted} />
        </View>
      </Pressable>
      <AppText variant="title" weight="700">
        {title}
      </AppText>
      {right ? <View style={{ marginLeft: 'auto' }}>{right}</View> : null}
    </View>
  );
}
