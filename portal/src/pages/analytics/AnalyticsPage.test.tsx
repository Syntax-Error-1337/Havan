import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { AnalyticsPage } from './AnalyticsPage';
import type { FleetStats } from '../../entities/fleet';

const stats = (over: Partial<FleetStats> = {}): FleetStats => ({
  devices: { total: 100, online: 42, bound: 90, unbound: 10 },
  new_devices: { last_7d: 7, last_30d: 21 },
  by_product: [{ product_key: 'hv_acme_bulb1', name: 'Acme Bulb E27', total: 100, online: 42 }],
  firmware: [
    { version: '2.0.0', devices: 60 },
    { version: null, devices: 40 },
  ],
  ...over,
});

function stubFetch(body: FleetStats) {
  vi.stubGlobal(
    'fetch',
    vi.fn(
      async () =>
        new Response(JSON.stringify(body), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
    ),
  );
}

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <AnalyticsPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('analytics', () => {
  it('counts the fleet and says what proportion is alive', async () => {
    stubFetch(stats());
    renderPage();

    expect(await screen.findByText('42 (42%)')).toBeTruthy();
    expect(screen.getByText('90')).toBeTruthy();
    expect(screen.getByText('10')).toBeTruthy();
  });

  it('names the firmware nobody has, rather than printing null', async () => {
    stubFetch(stats());
    renderPage();

    expect(await screen.findByText('never reported')).toBeTruthy();
  });

  it('does not divide by zero on an empty fleet', async () => {
    stubFetch(
      stats({
        devices: { total: 0, online: 0, bound: 0, unbound: 0 },
        by_product: [],
        firmware: [],
      }),
    );
    renderPage();

    expect(await screen.findByText('0 (0%)')).toBeTruthy();
    expect(screen.getByText(/No devices yet/i)).toBeTruthy();
  });
});
