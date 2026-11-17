import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { Providers } from '../src/app-shell/providers';
import { ErrorBoundary } from '../src/app-shell/ErrorBoundary';
import { useSession } from '../src/entities/session';
import { LoginPage } from '../src/pages/login/LoginPage';
import { useTheme } from '../src/app-shell/theme';

/**
 * The composition root. Routes stay dumb: they import one page and render it.
 * The auth gate lives here rather than per screen, so a new screen cannot
 * forget it.
 */
export default function RootLayout() {
  return (
    <Providers>
      <StatusBar style="auto" />
      <ErrorBoundary>
        <Gate />
      </ErrorBoundary>
    </Providers>
  );
}

function Gate() {
  const access = useSession((s) => s.access);
  // The screen's own background, from the *runtime* theme: this used to be the static `tokens` import,
  // which is the default palette rather than the operator's — so a light canvas painted itself behind
  // every card, and flipping the theme without a reload left the old colour in the gaps.
  const { color } = useTheme();

  if (!access) return <LoginPage />;

  return (
    <Stack
      screenOptions={{
        headerShown: false,
        contentStyle: { backgroundColor: color.bg },
        animation: 'slide_from_right',
      }}
    />
  );
}
