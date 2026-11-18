import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ApiError, download, request, setToken } from './client';

/**
 * What a failure looks like once it reaches a screen.
 *
 * These are the paths a page cannot test for itself: `request` decides whether the operator sees "the vendor is
 * suspended" or "Request failed (403)", and whether a 401 ends the session. Every page test stubs fetch and returns a
 * happy body, so nothing else in the suite exercises this.
 *
 * Written as `.test.ts` on purpose — and it did not run for the first year, because the vitest config only included
 * `*.test.tsx`. A pure module deserves a pure test file, so the config was widened rather than the file renamed.
 */

function respond(status: number, body: string, headers: Record<string, string> = {}) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => new Response(body, { status, headers: { 'content-type': 'application/json', ...headers } })),
  );
}

beforeEach(() => {
  setToken(null);
  localStorage.clear();
  vi.unstubAllGlobals();
});

describe('request', () => {
  it('carries the code, the message and the field details from the server', async () => {
    respond(
      409,
      JSON.stringify({
        error: {
          code: 'product_not_certified',
          message: 'this product is not certified yet',
          details: { product_key: 'hv_acme_bulb1' },
        },
      }),
    );

    await expect(request('/vendor/products/1/publish', { method: 'POST' })).rejects.toMatchObject({
      name: 'ApiError',
      status: 409,
      code: 'product_not_certified',
      message: 'this product is not certified yet',
      details: { product_key: 'hv_acme_bulb1' },
    });
  });

  it('says something honest when the failure is not JSON', async () => {
    // A proxy or a load balancer answering with HTML: the status is real, the body is not ours.
    respond(502, '<html><body>Bad gateway</body></html>', { 'content-type': 'text/html' });

    const error = (await request('/vendor/devices').catch((e: unknown) => e)) as ApiError;
    expect(error).toBeInstanceOf(ApiError);
    expect(error.status).toBe(502);
    expect(error.code).toBe('unknown');
    expect(error.message).toBe('Request failed (502)');
  });

  it('ends the session on a 401, so the gate can send the operator back to sign in', async () => {
    respond(401, JSON.stringify({ error: { code: 'unauthorized', message: 'missing bearer token' } }));

    await expect(request('/vendor/devices')).rejects.toBeInstanceOf(ApiError);
    expect(localStorage.getItem('haven.portal.token')).toBeNull();
  });

  it('does not clear a session for a failure that is not about the session', async () => {
    setToken('still-valid');
    respond(403, JSON.stringify({ error: { code: 'forbidden', message: 'only an owner can do that' } }));

    await expect(request('/vendor/team')).rejects.toBeInstanceOf(ApiError);
    expect(localStorage.getItem('haven.portal.token')).toBe('still-valid');
  });
});

describe('download', () => {
  it('repeats the server’s message rather than a status code when it has one', async () => {
    respond(409, JSON.stringify({ error: { code: 'conflict', message: 'that product has devices in homes' } }));

    const error = (await download('/vendor/support-bundle?device=1', 'bundle.json').catch((e: unknown) => e)) as ApiError;
    expect(error.message).toBe('that product has devices in homes');
    expect(error.code).toBe('download_failed');
  });

  it('falls back to the status when the error body is not JSON', async () => {
    respond(500, 'oops', { 'content-type': 'text/plain' });

    const error = (await download('/vendor/support-bundle?device=1', 'bundle.json').catch((e: unknown) => e)) as ApiError;
    expect(error.status).toBe(500);
    expect(error.message).toBe('download failed (500)');
  });
});
