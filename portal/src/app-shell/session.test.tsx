import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { MemoryRouter } from 'react-router-dom';

import { AppShell } from './AppShell';
import { SessionProvider } from './session';
import { setToken } from '../shared/api/client';

function stubFetch(status: number, body: unknown) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => ({
      ok: status < 400,
      status,
      text: async () => JSON.stringify(body),
    })) as unknown as typeof fetch,
  );
}

describe('session gate', () => {
  it('sends a vendor back to login when the API says the token is dead', async () => {
    // A token in localStorage that the server no longer accepts: the client
    // clears it on 401, and this asserts the gate actually notices. It used to
    // render the signed-in shell and print "missing bearer token" at the vendor.
    setToken('stale-token');
    stubFetch(401, { error: { code: 'unauthorized', message: 'missing bearer token' } });

    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <SessionProvider>
          <MemoryRouter initialEntries={['/']}>
            <AppShell />
          </MemoryRouter>
        </SessionProvider>
      </QueryClientProvider>,
    );

    expect(await screen.findByRole('button', { name: 'Sign in' })).toBeInTheDocument();
    await waitFor(() => expect(localStorage.getItem('haven.portal.token')).toBeNull());
  });
});
