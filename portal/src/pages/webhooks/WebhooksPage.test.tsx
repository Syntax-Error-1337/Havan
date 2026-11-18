import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { WebhooksPage } from './WebhooksPage';
import type { Webhook } from '../../entities/webhook';

const hook = (over: Partial<Webhook> = {}): Webhook => ({
  id: 'w-1',
  url: 'https://erp.example.test/haven',
  events: ['licence.activated'],
  active: true,
  created_at: '2026-11-01T09:00:00Z',
  last_delivery: null,
  ...over,
});

function stubFetch(hooks: Webhook[]) {
  const calls: { url: string; method: string }[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      calls.push({ url: String(input), method: init?.method ?? 'GET' });
      return new Response(JSON.stringify(hooks), {
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
      <MemoryRouter>
        <WebhooksPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('an endpoint and its last delivery', () => {
  it('says when nothing has been sent, rather than drawing a confident blank', async () => {
    stubFetch([hook()]);
    renderPage();
    // An endpoint that has never been sent to is not the same as one that has been failing; both are silent without this.
    expect(await screen.findByText('nothing yet')).toBeTruthy();
  });

  it('says how a failed delivery failed, and how many times', async () => {
    stubFetch([
      hook({
        last_delivery: {
          at: '2026-11-18T10:00:00Z',
          delivered: false,
          attempts: 3,
          error: '502 Bad Gateway',
        },
      }),
    ]);
    renderPage();
    expect(await screen.findByText(/failed 3 attempts: 502 Bad Gateway/)).toBeTruthy();
  });

  it('says a delivered one landed, and after how many tries', async () => {
    stubFetch([
      hook({
        last_delivery: { at: '2026-11-18T10:00:00Z', delivered: true, attempts: 2, error: null },
      }),
    ]);
    renderPage();
    expect(await screen.findByText(/delivered after 2 attempts/)).toBeTruthy();
  });
});

describe('removing an endpoint', () => {
  it('asks first, and says what stops', async () => {
    const calls = stubFetch([hook()]);
    const confirm = vi.fn(() => true);
    vi.stubGlobal('confirm', confirm);

    renderPage();
    await userEvent.click(await screen.findByRole('button', { name: 'Remove' }));

    expect(confirm).toHaveBeenCalledWith(expect.stringContaining('https://erp.example.test/haven'));
    await vi.waitFor(() =>
      expect(calls.some((c) => c.method === 'DELETE' && c.url.includes('w-1'))).toBe(true),
    );
  });

  it('does nothing when the person says no', async () => {
    const calls = stubFetch([hook()]);
    vi.stubGlobal('confirm', vi.fn(() => false));

    renderPage();
    await userEvent.click(await screen.findByRole('button', { name: 'Remove' }));

    // A confirmation that runs the action anyway is worse than no confirmation: it teaches people to press through.
    expect(calls.some((c) => c.method === 'DELETE')).toBe(false);
  });
});
