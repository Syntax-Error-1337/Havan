import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { FirmwarePage } from './FirmwarePage';
import { SessionProvider } from '../../app-shell/session';

const image = {
  id: 'fw-1',
  product_id: 'prod-1',
  version: '1.4.0',
  size: 812344,
  sha256: 'a'.repeat(64),
  ed25519_sig: 'c2ln',
  object_key: 'fw/acme/bulb/1.4.0.bin',
  created_at: '2026-01-01T00:00:00Z',
};

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
        <MemoryRouter initialEntries={['/products/prod-1/firmware']}>
          <Routes>
            <Route path="/products/:id/firmware" element={<FirmwarePage />} />
          </Routes>
        </MemoryRouter>
      </SessionProvider>
    </QueryClientProvider>,
  );
}

const product = {
  id: 'prod-1',
  vendor_id: 'v1',
  product_key: 'k',
  name: 'Acme Bulb',
  category: 'light',
  status: 'published' as const,
  schema_version: 1,
  thing_model: {},
  created_at: '2026-01-01T00:00:00Z',
};

describe('firmware', () => {
  it('lists what is registered, with the digest shortened', async () => {
    stubFetch(() => [image]);
    renderPage();

    expect(await screen.findByText('1.4.0')).toBeInTheDocument();
    expect(screen.getByText('793 KB')).toBeInTheDocument();
    expect(screen.getByText('aaaaaaaaaaaa…')).toBeInTheDocument();
  });

  it('refuses a digest that is not 64 hex characters before the round trip', async () => {
    stubFetch((url) => (url.includes('/firmware') ? [] : product));
    renderPage();

    await userEvent.click(await screen.findByRole('button', { name: 'Register image' }));
    fireEvent.change(screen.getByPlaceholderText('64 hex characters'), {
      target: { value: 'abc123' },
    });

    expect(screen.getByText(/a sha256 is 64 hex characters \(6 typed\)/)).toBeInTheDocument();
  });

  it('registers an image with everything the device needs to verify it', async () => {
    const calls = stubFetch((url) => (url.includes('/firmware') ? [] : product));
    renderPage();

    await userEvent.click(await screen.findByRole('button', { name: 'Register image' }));
    fireEvent.change(screen.getByPlaceholderText('1.4.0'), { target: { value: '1.5.0' } });
    fireEvent.change(screen.getByPlaceholderText('812344'), { target: { value: '1024' } });
    fireEvent.change(screen.getByPlaceholderText('64 hex characters'), {
      target: { value: 'b'.repeat(64) },
    });
    fireEvent.change(screen.getByPlaceholderText('base64, over the digest'), {
      target: { value: 'c2lnbmF0dXJl' },
    });
    fireEvent.change(screen.getByPlaceholderText('fw/acme/bulb/1.4.0.bin'), {
      target: { value: 'fw/acme/bulb/1.5.0.bin' },
    });
    await userEvent.click(screen.getByRole('button', { name: 'Register' }));

    await waitFor(() =>
      expect(calls.some((c) => c.method === 'POST' && c.url.includes('/firmware'))).toBe(true),
    );
    const post = calls.find((c) => c.method === 'POST');
    const body = JSON.parse(post?.body ?? '{}');
    expect(body).toMatchObject({
      version: '1.5.0',
      size: 1024,
      sha256: 'b'.repeat(64),
      ed25519_sig: 'c2lnbmF0dXJl',
      object_key: 'fw/acme/bulb/1.5.0.bin',
    });
  });

  it('says what a missing image means for a rollout', async () => {
    stubFetch(() => []);
    renderPage();

    expect(
      await screen.findByText(/A rollout needs at least one image\./),
    ).toBeInTheDocument();
  });
});
