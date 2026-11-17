import { SafeAreaView } from 'react-native-safe-area-context';
import type { ReactNode } from 'react';

import { Canvas } from './Canvas';

/** Screen shell: safe area, the gradient canvas, and nothing else. */
export function Screen({ children }: { children: ReactNode }) {
  return (
    <SafeAreaView style={{ flex: 1 }} edges={['top', 'left', 'right']}>
      <Canvas>{children}</Canvas>
    </SafeAreaView>
  );
}
