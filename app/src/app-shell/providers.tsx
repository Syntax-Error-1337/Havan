import { useEffect, type ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import { ActivityIndicator, View } from 'react-native';
import { ApiError } from '../shared/api/client';
import { ThemeProvider, tokens } from './theme';
import { installRefreshHandler, useSession } from '../entities/session';

/**
 * One query client for the whole app. Retries stop on a permanent 4xx — a 404
 * retried three times is just three more 404s.
 */
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 10_000,
      gcTime: 30 * 60_000,
      retry: (attempt, error) =>
        error instanceof ApiError ? !error.permanent && attempt < 2 : attempt < 2,
    },
    mutations: { retry: 0 },
  },
});

/**
 * Reads the keychain before the first meaningful frame: showing a logged-out
 * dashboard for 200 ms and then swapping it is worse than a spinner.
 */
function SessionBoot({ children }: { children: ReactNode }) {
  const hydrate = useSession((s) => s.hydrate);
  const hydrated = useSession((s) => s.hydrated);

  useEffect(() => {
    installRefreshHandler();
    void hydrate();
  }, [hydrate]);

  if (!hydrated) {
    return (
      <View
        style={{
          flex: 1,
          backgroundColor: tokens.color.bg,
          alignItems: 'center',
          justifyContent: 'center',
        }}
      >
        <ActivityIndicator color={tokens.color.accent} />
      </View>
    );
  }
  return <>{children}</>;
}

export function Providers({ children }: { children: ReactNode }) {
  return (
    <SafeAreaProvider>
      <QueryClientProvider client={queryClient}>
        <ThemeProvider>
          <SessionBoot>{children}</SessionBoot>
        </ThemeProvider>
      </QueryClientProvider>
    </SafeAreaProvider>
  );
}
