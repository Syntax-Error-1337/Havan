import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { LicencesPage } from './LicencesPage';
import type { LicencePage } from '../../entities/licence';

function stub(value: LicencePage) {
  vi.stubGlobal(
    'fetch',
    vi.fn(async () => new Response(JSON.stringify(value), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })),
  );
}

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <LicencesPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const page = (over: Partial<LicencePage['summary']> = {}): LicencePage => ({
  summary: {
    granted: 200,
    used: 151,
    remaining: 49,
    activated_this_period: 12,
    granted_value_cents: 49800,
    used_value_cents: 0,
    unit_price_cents: 249,
    currency: 'USD',
    ...over,
  },
  activations: [
    {
      serial: 'SN-ABC',
      product_key: 'hv_acme_bulb1',
      product_name: 'Acme Bulb E27',
      activated_at: '2026-11-01T10:00:00Z',
      unit_price_cents: 249,
    },
  ],
});

describe('LicencesPage', () => {
  it('shows the balance and what a grant cost', async () => {
    stub(page());
    renderPage();

    expect(await screen.findByText('200')).toBeDefined();
    expect(screen.getByText('49')).toBeDefined();
    // 200 granted at 249 cents each, 151 of them activated. The page must show the grant AND what the
    // activations actually consumed: "498.00 authorised" beside "151 activated" reads as an overcharge.
    expect(
      screen.getByText(/Granted value 498\.00 USD \(200 × 2\.49 USD\)/),
    ).toBeDefined();
    expect(screen.getByText(/consumed by 151 activations 375\.99 USD/)).toBeDefined();
    expect(screen.getByText(/unused 122\.01 USD/)).toBeDefined();
  });

  it('never hides a negative balance behind a zero', async () => {
    // What an operator revoking a grant after devices were live looks like. A screen that clamps
    // this to 0 shows a vendor something comfortable and untrue.
    stub(page({ granted: 100, used: 151, remaining: -51 }));
    renderPage();

    expect(await screen.findByText('-51')).toBeDefined();
    expect(screen.getByText(/the balance is\s+negative/)).toBeDefined();
  });

  it('lists the devices behind the number', async () => {
    stub(page());
    renderPage();

    expect(await screen.findByText('SN-ABC')).toBeDefined();
    expect(screen.getByText('Acme Bulb E27')).toBeDefined();
  });
});
