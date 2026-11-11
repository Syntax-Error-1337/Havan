import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { ProductEditorPage } from './ProductEditorPage';
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
  status: 'draft',
  schema_version: 1,
  thing_model: model,
  created_at: '2026-01-01T00:00:00Z',
  ...over,
});

function stubFetch(
  routes: (url: string, init?: RequestInit) => { status?: number; body: unknown },
) {
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

function renderEditor() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <SessionProvider>
        <MemoryRouter initialEntries={['/products/prod-1']}>
          <Routes>
            <Route path="/products/:id" element={<ProductEditorPage />} />
          </Routes>
        </MemoryRouter>
      </SessionProvider>
    </QueryClientProvider>,
  );
}

// By label, not by role: the page has a second textarea (the serial manifest).
const editor = () => screen.getByLabelText('Thing model') as HTMLTextAreaElement;

describe('thing-model editor', () => {
  it('loads the stored model into the document', async () => {
    stubFetch(() => ({ body: product() }));
    renderEditor();

    await waitFor(() => expect(editor().value).toContain('"product_key": "hv_acme_bulb1"'));
  });

  it('refuses to save a document that is not JSON, and says why', async () => {
    const calls = stubFetch(() => ({ body: product() }));
    renderEditor();
    await waitFor(() => expect(editor().value.length).toBeGreaterThan(0));

    // `fireEvent.change` rather than `userEvent.type`: braces and brackets are
    // key descriptors to user-event, not text.
    fireEvent.change(editor(), { target: { value: '{ not json' } });
    await userEvent.click(screen.getByRole('button', { name: 'Save draft' }));

    // A SyntaxError from JSON.parse is the vendor's typo, not a crash.
    expect(await screen.findByText(/Unexpected|Expected|token|position/i)).toBeInTheDocument();
    expect(calls.some((c) => c.method === 'PATCH')).toBe(false);
  });

  it('saves the parsed model, not the text', async () => {
    const calls = stubFetch(() => ({ body: product() }));
    renderEditor();
    await waitFor(() => expect(editor().value.length).toBeGreaterThan(0));

    fireEvent.change(editor(), {
      target: { value: JSON.stringify({ ...model, name: 'Acme Bulb v2' }, null, 2) },
    });
    await userEvent.click(screen.getByRole('button', { name: 'Save draft' }));

    await waitFor(() => expect(calls.some((c) => c.method === 'PATCH')).toBe(true));
    const patch = calls.find((c) => c.method === 'PATCH');
    expect(patch?.url).toContain('/vendor/products/prod-1');
    expect(JSON.parse(patch?.body ?? '{}').thing_model.name).toBe('Acme Bulb v2');
  });

  it('shows every failing rule at once', async () => {
    stubFetch((url) => {
      if (url.endsWith('/validate')) {
        return {
          body: {
            valid: false,
            violations: [
              { rule: 2, path: 'capabilities[0]', message: 'capability switch has no points' },
              { rule: 4, path: 'constraints', message: 'command_timeout_ms must be >= 1000' },
            ],
          },
        };
      }
      return { body: product() };
    });

    renderEditor();
    await waitFor(() => expect(editor().value.length).toBeGreaterThan(0));

    await userEvent.click(screen.getByRole('button', { name: 'Validate' }));

    expect(await screen.findByText(/capability switch has no points/)).toBeInTheDocument();
    expect(screen.getByText(/command_timeout_ms must be >= 1000/)).toBeInTheDocument();
    expect(screen.getByText('rule 2')).toBeInTheDocument();
    expect(screen.getByText('rule 4')).toBeInTheDocument();
  });

  it('locks a published product instead of letting a save fail', async () => {
    stubFetch(() => ({ body: product({ status: 'published' }) }));
    renderEditor();

    await waitFor(() => expect(editor().readOnly).toBe(true));
    expect(screen.getByRole('button', { name: 'Save draft' })).toBeDisabled();
    // Neither a fake "Published" button nor a Validate that could change nothing.
    expect(screen.queryByRole('button', { name: 'Published' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Publish' })).toBeNull();
    expect(screen.getByRole('button', { name: 'Validate' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Save draft' })).toBeDisabled();
  });
});
