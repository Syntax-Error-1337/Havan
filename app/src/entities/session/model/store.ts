import { registerForPush, unregisterPush } from '../../notifications/model/push';
import { create } from 'zustand';
import { secureStore } from '../../../shared/lib/secure';
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
  pushToken: string | null;
  setHome: (homeId: string) => Promise<void>;
};

export const useSession = create<SessionState>((set, get) => ({
  hydrated: false,
  access: null,
  refresh: null,
  homeId: null,
  pushToken: null,

  hydrate: async () => {
    const [access, refresh, homeId] = await Promise.all([
      secureStore.get(KEY_ACCESS),
      secureStore.get(KEY_REFRESH),
      secureStore.get(KEY_HOME),
    ]);
    setAccessToken(access);
    set({ access, refresh, homeId, hydrated: true });

    // A relaunch is the other half of signing in: the app was closed, and the platform still
    // needs to know which device to alert. Failure here is silent on purpose — push is not a
    // prerequisite for using the app.
    if (access) void refreshPushToken();
  },

  signIn: async (tokens) => {
    await Promise.all([
      secureStore.set(KEY_ACCESS, tokens.access),
      secureStore.set(KEY_REFRESH, tokens.refresh),
    ]);
    setAccessToken(tokens.access);
    set({ access: tokens.access, refresh: tokens.refresh });

    // After the token is in place: registration is an authenticated call. Not awaited by the
    // caller, so a phone that refuses permission does not delay the first screen.
    void refreshPushToken();
  },

  signOut: async () => {
    const refresh = get().refresh;
    // Remove this phone before the session goes: after it, the call has no credentials and the
    // next person to hold the phone keeps receiving the previous owner's alerts.
    await unregisterPush(get().pushToken);
    // Best effort: a failed network call must not trap the user in the app.
    if (refresh) {
      await request('/auth/logout', {
        method: 'POST',
        body: JSON.stringify({ refresh }),
      }).catch(() => undefined);
    }
    await Promise.all([
      secureStore.remove(KEY_ACCESS),
      secureStore.remove(KEY_REFRESH),
      secureStore.remove(KEY_HOME),
    ]);
    setAccessToken(null);
    set({ access: null, refresh: null, homeId: null, pushToken: null });
  },

  setHome: async (homeId) => {
    await secureStore.set(KEY_HOME, homeId);
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

/**
 * Register this phone for alerts, if it can be.
 *
 * Deliberately fire-and-forget: a refused permission, a browser build, or a flaky network must
 * not keep anyone out of their home. The token is kept in the store so signing out can remove
 * exactly the registration this session created.
 */
async function refreshPushToken(): Promise<void> {
  try {
    const result = await registerForPush();
    if (result.status === 'registered') {
      useSession.setState({ pushToken: result.token });
    }
  } catch {
    // Push unavailable is a normal state, not an error worth showing.
  }
}
