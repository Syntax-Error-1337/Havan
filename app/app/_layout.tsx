import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { Providers } from '../src/app/providers';
import { tokens } from '../src/shared/config/tokens';

/**
 * The composition root. Routes stay dumb: they import one page and render it.
 */
export default function RootLayout() {
  return (
    <Providers>
      <StatusBar style="light" />
      <Stack
        screenOptions={{
          headerShown: false,
          contentStyle: { backgroundColor: tokens.color.bg },
          animation: 'slide_from_right',
        }}
      />
    </Providers>
  );
}
