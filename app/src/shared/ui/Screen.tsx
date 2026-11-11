import { SafeAreaView } from 'react-native-safe-area-context';
import type { ReactNode } from 'react';
import { useTheme } from '../../app/theme';

/** Screen shell: safe area, background, and nothing else. */
export function Screen({ children }: { children: ReactNode }) {
  const { color } = useTheme();
  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: color.bg }} edges={['top', 'left', 'right']}>
      {children}
    </SafeAreaView>
  );
}
