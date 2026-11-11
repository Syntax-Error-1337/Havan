import { create } from 'zustand';
import * as SecureStore from 'expo-secure-store';
import { setAccessToken, setRefreshHandler } from '../../../shared/api/client';
import { request } from '../../../shared/api/client';

/**
 * The session lives in memory for rendering and in the keychain at rest. A
 * refresh token in AsyncStorage is a session steal on a rooted device; the
 * keychain is the only place it belongs.
 */

const KEY_ACCESS = 'haven.access';
const KEY_REFRESH = 'haven.refresh';
const KEY_HOME = 'haven.home';

export type SessionState = {
  hydrated: boolean;
  access: string | null;
  refresh: string | null;
  homeId: string | null;
  hydrate: () => Promise<void>;
  signIn: (tokens: { access: string; refresh: string }) => Promise<void>;
  signOut: () => Promise<void>;
  setHome: (homeId: string) => Promise<void>;
};

export const useSession = create<SessionState>((set, get) => ({
  hydrated: false,
  access: null,
  refresh: null,
  homeId: null,

  hydrate: async () => {
    const [access, refresh, homeId] = await Promise.all([
      SecureStore.getItemAsync(KEY_ACCESS),
      SecureStore.getItemAsync(KEY_REFRESH),
      SecureStore.getItemAsync(KEY_HOME),
    ]);
    setAccessToken(access);
    set({ access, refresh, homeId, hydrated: true });
  },

  signIn: async (tokens) => {
    await Promise.all([
      SecureStore.setItemAsync(KEY_ACCESS, tokens.access),
      SecureStore.setItemAsync(KEY_REFRESH, tokens.refresh),
    ]);
    setAccessToken(tokens.access);
    set({ access: tokens.access, refresh: tokens.refresh });
  },

  signOut: async () => {
    const refresh = get().refresh;
    // Best effort: a failed network call must not trap the user in the app.
    if (refresh) {
      await request('/auth/logout', {
        method: 'POST',
        body: JSON.stringify({ refresh }),
      }).catch(() => undefined);
    }
    await Promise.all([
      SecureStore.deleteItemAsync(KEY_ACCESS),
      SecureStore.deleteItemAsync(KEY_REFRESH),
      SecureStore.deleteItemAsync(KEY_HOME),
    ]);
    setAccessToken(null);
    set({ access: null, refresh: null, homeId: null });
  },

  setHome: async (homeId) => {
    await SecureStore.setItemAsync(KEY_HOME, homeId);
    set({ homeId });
  },
}));

/**
 * Rotate the access token once per 401. Wired here rather than in the client so
 * `shared/api` stays free of session policy.
 */
export function installRefreshHandler() {
  setRefreshHandler(async () => {
    const refresh = useSession.getState().refresh;
    if (!refresh) return null;

    const res = await fetch(
      `${process.env.EXPO_PUBLIC_API_URL ?? 'http://localhost:8080/v1'}/auth/refresh`,
      {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ refresh }),
      },
    );

    if (!res.ok) {
      // A revoked family means the session is gone; drop it and let the gate
      // send the user back to login.
      await useSession.getState().signOut();
      return null;
    }

    const tokens = (await res.json()) as { access: string; refresh: string };
    await useSession.getState().signIn(tokens);
    return tokens.access;
  });
}
