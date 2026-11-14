import { Platform } from 'react-native';

import { request } from '../../../shared/api/client';

/**
 * Registering this phone for alerts.
 *
 * The platform side has been able to deliver push since the sender landed, but nothing ever
 * told it which device to deliver to — so on a real phone every alert still stopped at the
 * websocket. This is the missing half.
 *
 * Three rules, each of which is a real way to get this wrong:
 *
 * 1. **Ask before asking.** `getDevicePushTokenAsync` on iOS throws if permission was never
 *    requested, so permission comes first and a refusal ends it.
 * 2. **A refusal is an answer, not an error.** Someone who declined alerts does not get a
 *    second prompt on every launch; the settings screen says what it is and offers a retry.
 * 3. **The token is not a secret but it is per-user.** Signing out removes it, or the next
 *    person holding the phone receives the previous owner's alerts.
 */

export type PushRegistration =
  | { status: 'registered'; token: string }
  | { status: 'denied' }
  | { status: 'unsupported'; reason: string };

/** Where a device token would come from. Injected so the tests do not need a native module. */
export type PushSource = {
  permission: () => Promise<{ granted: boolean }>;
  request: () => Promise<{ granted: boolean }>;
  deviceToken: () => Promise<string>;
};

/**
 * The real source, loaded lazily.
 *
 * Lazily because `expo-notifications` throws on import in a browser — and the app is served in
 * a browser on this machine (`make app-web`). A static import would take the whole app down on
 * web, which is exactly the sort of thing that looks like "the app is broken" rather than
 * "push is not available here".
 */
async function nativeSource(): Promise<PushSource | null> {
  if (Platform.OS !== 'ios' && Platform.OS !== 'android') return null;

  const Notifications = await import('expo-notifications');
  return {
    permission: async () => {
      const { status } = await Notifications.getPermissionsAsync();
      return { granted: status === 'granted' };
    },
    request: async () => {
      const { status } = await Notifications.requestPermissionsAsync();
      return { granted: status === 'granted' };
    },
    deviceToken: async () => (await Notifications.getDevicePushTokenAsync()).data,
  };
}

/**
 * Ask for permission if needed, then hand the token to the platform.
 *
 * Returns what happened instead of throwing: a phone that declined notifications is a normal
 * state, not a failure the sign-in flow should surface.
 */
export async function registerForPush(
  source?: PushSource,
): Promise<PushRegistration> {
  const push = source ?? (await nativeSource());
  if (!push) return { status: 'unsupported', reason: 'not a phone build' };

  let allowed = await push.permission();
  if (!allowed.granted) {
    // Only ask when the person has not answered yet — iOS reports `undetermined` once and the
    // prompt is expensive: it can only be shown a limited number of times.
    allowed = await push.request();
  }
  if (!allowed.granted) return { status: 'denied' };

  const token = await push.deviceToken();
  if (!token) return { status: 'unsupported', reason: 'no device token' };

  await request('/notifications/tokens', {
    method: 'POST',
    body: JSON.stringify({
      platform: Platform.OS === 'ios' ? 'ios' : 'android',
      token,
      app_version: process.env.EXPO_PUBLIC_APP_VERSION ?? undefined,
    }),
  });

  return { status: 'registered', token };
}

/** Sign-out: stop this phone from receiving alerts for the account that just left. */
export async function unregisterPush(token: string | null): Promise<void> {
  if (!token) return;
  try {
    await request(`/notifications/tokens/${encodeURIComponent(token)}`, { method: 'DELETE' });
  } catch {
    // A phone that is offline while signing out is common; the token is pruned server-side
    // when the provider reports it dead, so failing here must not block the sign-out.
  }
}
