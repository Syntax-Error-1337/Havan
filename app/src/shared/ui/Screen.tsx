import { SafeAreaView } from 'react-native-safe-area-context';
import type { ReactNode } from 'react';

import { MeshBackground } from './MeshBackground';

/** Screen shell: safe area, the mesh canvas, and nothing else. */
export function Screen({ children }: { children: ReactNode }) {
  return (
    <SafeAreaView style={{ flex: 1 }} edges={['top', 'left', 'right']}>
      <MeshBackground />
      {children}
    </SafeAreaView>
  );
}
