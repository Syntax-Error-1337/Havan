import { Platform } from 'react-native';

import { registerForPush, unregisterPush, type PushSource } from './push';

const granted = { granted: true };
const denied = { granted: false };

function source(over: Partial<PushSource> = {}): PushSource {
  return {
    permission: async () => granted,
    request: async () => granted,
    deviceToken: async () => 'dev-token-1234567890',
    ...over,
  };
}

function stubFetch() {
  const calls: { url: string; method: string; body?: string }[] = [];
  globalThis.fetch = jest.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    calls.push({ url: String(input), method: init?.method ?? 'GET', body: init?.body as string });
    return new Response(null, { status: 204 });
  }) as unknown as typeof fetch;
  return calls;
}

describe('push registration', () => {
  beforeEach(() => {
    Platform.OS = 'ios';
  });

  it('sends the device token to the platform', async () => {
    const calls = stubFetch();

    const result = await registerForPush(source());

    expect(result).toEqual({ status: 'registered', token: 'dev-token-1234567890' });
    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toContain('/notifications/tokens');
    expect(calls[0]?.method).toBe('POST');
    expect(JSON.parse(calls[0]?.body ?? '{}')).toMatchObject({
      platform: 'ios',
      token: 'dev-token-1234567890',
    });
  });

  it('does not nag someone who already said no', async () => {
    const calls = stubFetch();
    const asked: string[] = [];

    const result = await registerForPush(
      source({
        permission: async () => {
          asked.push('permission');
          return denied;
        },
        request: async () => {
          asked.push('request');
          return denied;
        },
      }),
    );

    expect(result).toEqual({ status: 'denied' });
    expect(calls).toHaveLength(0);
    // One prompt is the most a refusal earns — iOS only shows it a limited number of times.
    expect(asked).toEqual(['permission', 'request']);
  });

  it('asks once when permission was never requested', async () => {
    const calls = stubFetch();

    const result = await registerForPush(
      source({
        permission: async () => denied,
        request: async () => granted,
      }),
    );

    expect(result.status).toBe('registered');
    expect(calls).toHaveLength(1);
  });

  it('reports no token rather than sending an empty one', async () => {
    const calls = stubFetch();

    const result = await registerForPush(source({ deviceToken: async () => '' }));

    expect(result).toEqual({ status: 'unsupported', reason: 'no device token' });
    expect(calls).toHaveLength(0);
  });

  it('does nothing in a browser, where there is no push', async () => {
    Platform.OS = 'web';
    const calls = stubFetch();

    const result = await registerForPush();

    expect(result.status).toBe('unsupported');
    expect(calls).toHaveLength(0);
  });

  it('removes the token on sign-out so the next owner gets nothing', async () => {
    const calls = stubFetch();

    await unregisterPush('dev-token-1234567890');

    expect(calls[0]?.method).toBe('DELETE');
    expect(calls[0]?.url).toContain('/notifications/tokens/dev-token-1234567890');
  });

  it('still signs out when the delete fails', async () => {
    globalThis.fetch = jest.fn(async () => {
      throw new TypeError('Network request failed');
    }) as unknown as typeof fetch;

    // Signing out must not depend on the network; the server prunes dead tokens anyway.
    await expect(unregisterPush('dev-token-1234567890')).resolves.toBeUndefined();
  });
});
