import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { InvoicesPage, priceGroups } from './InvoicesPage';
import type { Invoice } from '../../entities/invoice';

function stub(value: Invoice) {
  vi.stubGlobal(
    'fetch',
    vi.fn(
      async () =>
        new Response(JSON.stringify(value), {
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
        <InvoicesPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const line = (serial: string, unit_price_cents: number): Invoice['lines'][number] => ({
  serial,
  product_key: 'hv_acme_bulb1',
  unit_price_cents,
  activated_at: '2026-11-12T03:05:50.294256Z',
});

/**
 * The only screen a vendor can check our arithmetic on.
 *
 * The case that matters is the mixed month: some devices are activated against a priced grant and some
 * against a free one, so the table is mostly zeroes and the total is not — which reads as a pricing bug
 * unless the document reconciles on screen.
 */
const mixed: Invoice = {
  period: '2026-11',
  currency: 'USD',
  quantity: 4,
  amount_cents: 498,
  status: 'draft',
  issued_at: null,
  lines: [line('A-1', 0), line('A-2', 0), line('B-1', 249), line('B-2', 249)],
};

describe('InvoicesPage', () => {
  it('reconciles the total from the rows, and names the free ones', async () => {
    stub(mixed);
    renderPage();

    const reconciliation = await screen.findByTestId('invoice-reconciliation');
    // 2 x 2.49 = 4.98; 2 free. Both numbers say what they are.
    expect(reconciliation.textContent).toContain('2 devices at 2.49 USD = 4.98 USD');
    expect(reconciliation.textContent).toContain('2 devices at no charge');
    expect(reconciliation.textContent).toContain('total 4.98 USD');
  });

  it('never prints a bare zero where a device was free', async () => {
    stub(mixed);
    renderPage();

    // "0.00 USD" is indistinguishable from a device that failed to bill.
    expect((await screen.findAllByText('No charge')).length).toBe(2);
    expect(screen.queryByText('0.00 USD')).toBeNull();
  });

  it('closes the table with the same total the header shows', async () => {
    stub(mixed);
    renderPage();

    const footer = await screen.findByText(/Total · 4 devices/);
    expect(footer.closest('tr')?.textContent).toContain('4.98 USD');
  });

  it('groups by price, dearest first', async () => {
    // Pure arithmetic, tested without a renderer: the strip is only worth having if it adds up.
    expect(priceGroups(mixed.lines)).toEqual([
      { price: 249, count: 2 },
      { price: 0, count: 2 },
    ]);
  });
});
