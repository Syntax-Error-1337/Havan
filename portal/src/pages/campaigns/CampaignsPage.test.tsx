import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { CampaignsPage } from './CampaignsPage';
import { SessionProvider } from '../../app-shell/session';
import type { CampaignRow } from '../../entities/ota';

const product = {
  id: 'prod-1',
  vendor_id: 'v1',
  product_key: 'hv_acme_bulb1',
  name: 'Acme Bulb',
  category: 'light',
  status: 'published' as const,
  schema_version: 1,
  thing_model: {
    schema_version: 1,
    product_key: 'hv_acme_bulb1',
    name: 'Acme Bulb',
    category: 'light',
    capabilities: [],
  },
  created_at: '2026-01-01T00:00:00Z',
};

const row = (over: Partial<CampaignRow['campaign']> = {}, progress = {}): CampaignRow => ({
  campaign: {
    id: 'camp-1',
    vendor_id: 'v1',
    product_id: 'prod-1',
    firmware_id: 'fw-1',
    rollout_pct: 50,
    status: 'running',
    created_at: '2026-01-01T00:00:00Z',
    started_at: '2026-01-01T00:05:00Z',
    ...over,
  },
  progress: {
    total: 10,
    pending: 0,
    sent: 0,
    downloading: 0,
    verified: 0,
    rebooting: 0,
    success: 7,
    failed: 3,
    skipped: 0,
    ...progress,
  },
  complete: false,
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
        <MemoryRouter initialEntries={['/campaigns']}>
          <CampaignsPage />
        </MemoryRouter>
      </SessionProvider>
    </QueryClientProvider>,
  );
}

describe('rollouts', () => {
  it('leads with failures rather than a percentage', async () => {
    stubFetch((url) => (url.endsWith('/vendor/campaigns') ? [row()] : [product]));
    renderPage();

    expect(await screen.findByText('Acme Bulb')).toBeInTheDocument();
    expect(screen.getByText('50%')).toBeInTheDocument();
    // "70% done" hides that three devices are broken; the count does not.
    expect(screen.getByText('3 failed')).toBeInTheDocument();
  });

  it('says plainly when a rollout matched nobody', async () => {
    stubFetch((url) =>
      url.endsWith('/vendor/campaigns')
        ? [row({ status: 'draft' }, { total: 0, success: 0, failed: 0 })]
        : [product],
    );
    renderPage();

    expect(await screen.findByText('Matched no devices')).toBeInTheDocument();
  });

  it('starts a draft through the API', async () => {
    const calls = stubFetch((url) => {
      if (url.endsWith('/start')) return { queued: 4 };
      return url.endsWith('/vendor/campaigns')
        ? [row({ status: 'draft' }, { total: 0, success: 0, failed: 0 })]
        : [product];
    });
    renderPage();

    await userEvent.click(await screen.findByRole('button', { name: 'Start' }));

    await waitFor(() => expect(calls.some((c) => c.url.endsWith('/start'))).toBe(true));
    expect(calls.find((c) => c.url.endsWith('/start'))?.method).toBe('POST');
  });

  it('offers no Start button once a rollout is running', async () => {
    stubFetch((url) => (url.endsWith('/vendor/campaigns') ? [row()] : [product]));
    renderPage();

    await screen.findByText('Acme Bulb');
    expect(screen.queryByRole('button', { name: 'Start' })).not.toBeInTheDocument();
  });
});
