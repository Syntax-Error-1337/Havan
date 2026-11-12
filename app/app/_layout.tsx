import { Stack } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { Providers } from '../src/app-shell/providers';
import { ErrorBoundary } from '../src/app-shell/ErrorBoundary';
import { useSession } from '../src/entities/session';
import { LoginPage } from '../src/pages/login/LoginPage';
import { tokens } from '../src/shared/config/tokens';

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

  if (!access) return <LoginPage />;

  return (
    <Stack
      screenOptions={{
        headerShown: false,
        contentStyle: { backgroundColor: tokens.color.bg },
        animation: 'slide_from_right',
      }}
    />
  );
}
