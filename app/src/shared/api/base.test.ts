/**
 * Where the client sends requests.
 *
 * The bug this locks down: the base used to be a hard `localhost`, which is correct on a
 * simulator and wrong on a phone — a phone loading the bundle from `exp://192.168.0.49:8081`
 * has its own localhost, so every request went nowhere and the app simply could not sign in.
 */
import { describe, expect, it, jest } from '@jest/globals';

function baseWith(hostUri: string | undefined, explicit?: string) {
  jest.resetModules();
  jest.doMock('expo-constants', () => ({
    __esModule: true,
    default: {
      expoConfig: hostUri ? { hostUri } : null,
      expoGoConfig: undefined,
    },
  }));
  if (explicit) process.env.EXPO_PUBLIC_API_URL = explicit;
  else delete process.env.EXPO_PUBLIC_API_URL;

  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const client = require('./client') as typeof import('./client');
  return client.BASE;
}

describe('the API base', () => {
  it('follows the host Metro served the bundle from', () => {
    expect(baseWith('192.168.0.49:8081')).toBe('http://192.168.0.49:8080/v1');
  });

  it('stays on localhost for a simulator', () => {
    expect(baseWith('localhost:8081')).toBe('http://localhost:8080/v1');
  });

  it('lets an explicit URL win', () => {
    expect(baseWith('192.168.0.49:8081', 'https://api.example.test/v1')).toBe(
      'https://api.example.test/v1',
    );
  });

  it('falls back to localhost when nothing knows the host', () => {
    expect(baseWith(undefined)).toBe('http://localhost:8080/v1');
  });
});
