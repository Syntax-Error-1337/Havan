import { useEffect } from 'react';
import { Stack } from 'expo-router';
import * as SplashScreen from 'expo-splash-screen';
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
// Held until `SessionBoot` has an answer. Without this the OS splash drops at the first frame — before the
// keychain is read — so a signed-in customer sees the login form for a moment on every cold start, which
// looks like being signed out. A splash for 200 ms longer is better than that flicker.
void SplashScreen.preventAutoHideAsync().catch(() => undefined);

export default function RootLayout() {
  // The web's static first paint (`+html.tsx`) is a branded canvas rather than a white page. React is
  // mounted by the time this runs, so the placeholder has done its job — fade it out and drop it.
  useEffect(() => {
    if (typeof document === 'undefined') return;
    const boot = document.getElementById('boot');
    if (!boot) return;
    boot.dataset.gone = '1';
    const timer = setTimeout(() => boot.remove(), 260);
    return () => clearTimeout(timer);
  }, []);

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
