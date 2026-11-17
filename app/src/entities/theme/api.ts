import { useQuery } from '@tanstack/react-query';
import * as SecureStore from 'expo-secure-store';

import { request } from '../../shared/api/client';

/**
 * The platform theme: colours and branding the operator sets, not constants we ship.
 *
 * Three rules make this safe to put in front of a customer:
 *
 * 1. **It can never blank the app.** The built-in palette is the default and the fallback; a missing
 *    endpoint, a 500, a flight-mode start and a malformed payload all resolve to it.
 * 2. **It survives a cold start offline.** The last good answer is cached locally and used before the
 *    first paint, so a phone coming out of a tunnel does not flash the wrong brand.
 * 3. **The server decides what is readable.** `accent_on` arrives computed; the client never guesses
 *    a text colour, because two clients guessing is two different answers.
 */
export type PlatformTheme = {
  product_name: string;
  logo_text: string;
  accent: string;
  accent_secondary: string;
  mode: 'dark' | 'light';
  accent_on: string;
  updated_at: string | null;
  updated_by: string | null;
};

/** What the app wears when nobody has said otherwise. Matches the server's seeded row. */
export const DEFAULT_THEME: PlatformTheme = {
  product_name: 'Haven',
  logo_text: 'HV',
  accent: '#0A84FF',
  accent_secondary: '#8B5CF6',
  mode: 'dark',
  accent_on: '#000000',
  updated_at: null,
  updated_by: null,
};

/**
 * The theme as last seen, kept in memory so the very first render already wears it.
 *
 * `undefined` means "not read yet": the provider paints the built-in theme until this resolves, which
 * is one frame rather than a white flash.
 */
const STORE_KEY = 'haven_theme_v1';

let cached: PlatformTheme | null = null;

export function cachedTheme(): PlatformTheme | null {
  return cached;
}

/** Load the cached theme at start-up. Called once, before the tree needs it. */
export async function hydrateThemeCache(): Promise<PlatformTheme | null> {
  if (cached) return cached;
  try {
    const raw = await SecureStore.getItemAsync(STORE_KEY);
    if (raw) {
      cached = { ...DEFAULT_THEME, ...(JSON.parse(raw) as Partial<PlatformTheme>) };
    }
  } catch {
    // A corrupt cache is not a reason to fail a start: the network answer replaces it.
  }
  return cached;
}

export function usePlatformTheme() {
  return useQuery({
    queryKey: ['platform-theme'],
    queryFn: async () => {
      const theme = await request<PlatformTheme>('/theme');
      cached = theme;
      // Fire and forget: the cache is an optimisation, and a failed write must not fail the read.
      SecureStore.setItemAsync(STORE_KEY, JSON.stringify(theme)).catch(() => {});
      return theme;
    },
    initialData: cached ?? undefined,
    // A theme changes when an operator decides to change it, which is rare; an hour of staleness
    // costs nothing and stops every launch from asking.
    staleTime: 60 * 60 * 1000,
    retry: 1,
  });
}
