import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { ProductsPage } from './ProductsPage';
import { SessionProvider } from '../../app-shell/session';
import type { Product } from '../../entities/product';

const model = {
  schema_version: 1,
  product_key: 'hv_acme_bulb1',
  name: 'Acme Bulb',
  category: 'light',
  capabilities: [{ name: 'switch', points: { power: { dp: '1', kind: 'bool' } } }],
};

const product = (over: Partial<Product> = {}): Product => ({
  id: 'prod-1',
  vendor_id: 'v1',
  product_key: 'hv_acme_bulb1',
  name: 'Acme Bulb',
  category: 'light',
  status: 'published',
  schema_version: 1,
  thing_model: model,
  created_at: '2026-01-01T00:00:00Z',
  ...over,
});

function stubFetch(routes: (url: string, init?: RequestInit) => { status?: number; body: unknown }) {
  const calls: { url: string; method: string; body?: string }[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      calls.push({ url, method: init?.method ?? 'GET', body: init?.body as string });
      const { status = 200, body } = routes(url, init);
      return new Response(JSON.stringify(body), {
        status,
        headers: { 'content-type': 'application/json' },
      });
    }),
  );
  return calls;
}

function renderProducts() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <SessionProvider>
        <MemoryRouter initialEntries={['/']}>
          <Routes>
            <Route path="/" element={<ProductsPage />} />
            <Route path="/products/:id" element={<p>editor screen</p>} />
          </Routes>
        </MemoryRouter>
      </SessionProvider>
    </QueryClientProvider>,
  );
}

describe('products', () => {
  it('lists the catalogue with status, and says what publishing gates', async () => {
    stubFetch(() => ({ body: [product(), product({ id: 'prod-2', name: 'Acme Plug', status: 'draft' })] }));
    renderProducts();

    expect(await screen.findByText('Acme Bulb')).toBeInTheDocument();
    expect(screen.getByText('Acme Plug')).toBeInTheDocument();
    expect(screen.getByText(/must be published before a customer can pair it/)).toBeInTheDocument();
    // Status is the column vendors scan for; both must be distinguishable.
    expect(screen.getByText('published')).toBeInTheDocument();
    expect(screen.getByText('draft')).toBeInTheDocument();
  });

  it('counts the catalogue, singular and plural', async () => {
    stubFetch(() => ({ body: [product()] }));
    renderProducts();

    expect(await screen.findByText(/1 product ·/)).toBeInTheDocument();
  });

  it('creates a draft from two fields and opens it', async () => {
    const calls = stubFetch((_url, init) =>
      init?.method === 'POST' ? { body: product({ id: 'new-1' }) } : { body: [] },
    );
    renderProducts();

    await userEvent.click(await screen.findByRole('button', { name: 'New product' }));
    fireEvent.change(screen.getByPlaceholderText('hv_acme_strip1'), {
      target: { value: 'hv_acme_new' },
    });
    fireEvent.change(screen.getByPlaceholderText('Acme Light Strip'), {
      target: { value: 'Acme Light' },
    });
    await userEvent.click(screen.getByRole('button', { name: 'Create draft' }));

    await waitFor(() => expect(calls.some((c) => c.method === 'POST')).toBe(true));
    const post = calls.find((c) => c.method === 'POST');
    const body = JSON.parse(post?.body ?? '{}');
    expect(body.thing_model.product_key).toBe('hv_acme_new');
    // A product is never born published; publishing is a separate, deliberate act.
    expect(body.publish).toBe(false);
    expect(await screen.findByText('editor screen')).toBeInTheDocument();
  });
});
