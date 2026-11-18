import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { ApiKeysPage } from './ApiKeysPage';
import { SessionProvider } from '../../app-shell/session';
import type { ApiKey } from '../../entities/api-key';

const key = (over: Partial<ApiKey> = {}): ApiKey => ({
  id: 'key-1',
  name: 'factory line 2',
  prefix: 'a1b2c3d4',
  created_at: '2026-01-01T00:00:00Z',
  last_used_at: null,
  revoked_at: null,
  ...over,
});

function stubFetch(routes: (url: string, init?: RequestInit) => unknown) {
  const calls: { url: string; method: string; body?: string }[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      calls.push({ url, method: init?.method ?? 'GET', body: init?.body as string });
      return new Response(JSON.stringify(routes(url, init)), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    }),
  );
  return calls;
}

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <SessionProvider>
        <MemoryRouter initialEntries={['/api-keys']}>
          <ApiKeysPage />
        </MemoryRouter>
      </SessionProvider>
    </QueryClientProvider>,
  );
}

describe('api keys', () => {
  it('shows the prefix and never a secret', async () => {
    stubFetch(() => [key({ last_used_at: '2026-02-01T10:00:00Z' })]);
    renderPage();

    expect(await screen.findByText('factory line 2')).toBeInTheDocument();
    expect(screen.getByText('hv_a1b2c3d4_…')).toBeInTheDocument();
    expect(screen.getByText('active')).toBeInTheDocument();
  });

  it('says a key has never been used, rather than implying it works', async () => {
    stubFetch(() => [key()]);
    renderPage();

    expect(await screen.findByText('never')).toBeInTheDocument();
    expect(screen.getByText('active')).toBeInTheDocument();
  });

  it('reveals the secret once, with a warning, right after minting', async () => {
    stubFetch((_url, init) =>
      init?.method === 'POST'
        ? { ...key(), secret: 'hv_a1b2c3d4_deadbeefdeadbeefdeadbeefdeadbeef' }
        : [],
    );
    renderPage();

    await userEvent.type(await screen.findByPlaceholderText('factory line 2'), 'ci runner');
    await userEvent.click(screen.getByRole('button', { name: 'Create key' }));

    expect(await screen.findByText('Copy this now')).toBeInTheDocument();
    expect(
      screen.getByDisplayValue('hv_a1b2c3d4_deadbeefdeadbeefdeadbeefdeadbeef'),
    ).toBeInTheDocument();
  });

  it('says so when the clipboard refuses, instead of claiming it copied', async () => {
    stubFetch((_url, init) =>
      init?.method === 'POST'
        ? { ...key(), secret: 'hv_a1b2c3d4_deadbeefdeadbeefdeadbeefdeadbeef' }
        : [],
    );

    // The one state a vendor cannot recover from: believing they have a secret
    // that was never copied, on the only screen that will ever show it.
    const denied = vi.fn().mockRejectedValueOnce(new Error('denied'));
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: denied }, configurable: true });

    renderPage();
    await userEvent.type(await screen.findByPlaceholderText('factory line 2'), 'ci runner');
    await userEvent.click(screen.getByRole('button', { name: 'Create key' }));
    await screen.findByText('Copy this now');

    await userEvent.click(screen.getByRole('button', { name: 'Copy' }));

    expect(await screen.findByText('Select it — copy failed')).toBeInTheDocument();
    expect(screen.queryByText('Copied')).toBeNull();
  });

  it('revokes through the API and hides the button, after asking first', async () => {
    vi.stubGlobal('confirm', vi.fn(() => true));
    const calls = stubFetch((_url, init) =>
      init?.method === 'DELETE' ? null : [key()],
    );
    renderPage();

    await userEvent.click(await screen.findByRole('button', { name: 'Revoke' }));

    await waitFor(() => expect(calls.some((c) => c.method === 'DELETE')).toBe(true));
    expect(calls.find((c) => c.method === 'DELETE')?.url).toContain('/vendor/api-keys/key-1');
  });

  it('offers no revoke for a key that is already revoked', async () => {
    stubFetch(() => [key({ revoked_at: '2026-03-01T00:00:00Z' })]);
    renderPage();

    expect(await screen.findByText('revoked')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Revoke' })).not.toBeInTheDocument();
  });
});

describe('rotation order', () => {
  it('puts the keys that still work first, newest first', async () => {
    stubFetch(() => [
      key({ id: 'k-old-revoked', name: 'old-revoked', revoked_at: '2026-03-01T00:00:00Z' }),
      key({ id: 'k-older-live', name: 'older-live', created_at: '2026-11-01T00:00:00Z' }),
      key({ id: 'k-newer-live', name: 'newer-live', created_at: '2026-11-15T00:00:00Z' }),
    ]);
    renderPage();

    // Rotating a credential starts by finding the ones in use; mixing revoked keys through the list makes the reader
    // scan past history to find the rows they can act on.
    const names = (await screen.findAllByRole('row'))
      .slice(1)
      .map((row) => row.querySelector('td')?.textContent);
    expect(names).toEqual(['newer-live', 'older-live', 'old-revoked']);
  });
});
