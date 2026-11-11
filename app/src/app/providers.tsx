import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import type { ReactNode } from 'react';
import { ApiError } from '../shared/api/client';
import { ThemeProvider } from './theme';

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

export function Providers({ children }: { children: ReactNode }) {
  return (
    <SafeAreaProvider>
      <QueryClientProvider client={queryClient}>
        <ThemeProvider>{children}</ThemeProvider>
      </QueryClientProvider>
    </SafeAreaProvider>
  );
}
