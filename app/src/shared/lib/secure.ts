import { Platform } from 'react-native';
import * as SecureStore from 'expo-secure-store';

/**
 * Where the session lives at rest.
 *
 * On a phone: the keychain, because a refresh token in plain storage is a
 * session steal on a rooted device. On the web build there is no keychain, and
 * `expo-secure-store` throws rather than degrading — so the web build uses
 * localStorage and the app is honest about being a development surface.
 *
 * This is the only module that knows which of the two is in use.
 */
const isWeb = Platform.OS === 'web';

async function get(key: string): Promise<string | null> {
  if (isWeb) {
    return globalThis.localStorage?.getItem(key) ?? null;
  }
  return SecureStore.getItemAsync(key);
}

async function set(key: string, value: string): Promise<void> {
  if (isWeb) {
    globalThis.localStorage?.setItem(key, value);
    return;
  }
  await SecureStore.setItemAsync(key, value);
}

async function remove(key: string): Promise<void> {
  if (isWeb) {
    globalThis.localStorage?.removeItem(key);
    return;
  }
  await SecureStore.deleteItemAsync(key);
}

export const secureStore = { get, set, remove, isWeb };
