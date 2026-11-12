import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { FleetPage, ago } from './FleetPage';
import type { FleetPage as Page } from '../../entities/fleet';

const device = (over: Partial<Page['devices'][number]> = {}): Page['devices'][number] => ({
  id: 'd1',
  product_key: 'hv_acme_bulb1',
  product_name: 'Acme Bulb E27',
  name: 'Kitchen bulb',
  serial: 'SN-1',
  state: 'claimed',
  bound: true,
  online: true,
  fw_version: '1.0.0',
  last_seen_at: new Date().toISOString(),
  created_at: '2026-01-01T00:00:00Z',
  ...over,
});

function stubFetch(handler: (url: string) => Page) {
  const urls: string[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      urls.push(url);
      return new Response(JSON.stringify(handler(url)), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    }),
  );
  return urls;
}

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <FleetPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('fleet', () => {
  it('lists devices with their status, and no customer identity', async () => {
    stubFetch(() => ({ devices: [device()], next_cursor: null }));
    renderPage();

    expect(await screen.findByText('SN-1')).toBeTruthy();
    expect(screen.getByText('Acme Bulb E27')).toBeTruthy();
    expect(screen.getByText('Kitchen bulb')).toBeTruthy();
    expect(screen.getByTestId('fleet-state').textContent).toBe('online');

    // The vendor sells hardware; who bought it is not theirs to see. Asserted on the
    // columns rather than on the word "home", which appears in prose all over the page.
    const headers = Array.from(document.querySelectorAll('th')).map((h) => h.textContent);
    expect(headers).toEqual(['Serial', 'Product', 'Name', 'Status', 'Firmware', 'Last seen']);
  });

  it('says so when the fleet is empty, instead of showing an empty table', async () => {
    stubFetch(() => ({ devices: [], next_cursor: null }));
    renderPage();

    expect(await screen.findByText(/No devices match/i)).toBeTruthy();
  });

  it('asks the server for the state it was given', async () => {
    const urls = stubFetch(() => ({ devices: [], next_cursor: null }));
    renderPage();

    await userEvent.selectOptions(await screen.findByLabelText('Filter by state'), 'unbound');

    await waitFor(() => {
      expect(urls.some((u) => u.includes('state=unbound'))).toBe(true);
    });
  });

  it('offers the next page only when the server says there is one', async () => {
    stubFetch((url) =>
      url.includes('cursor=')
        ? { devices: [device({ id: 'd2', serial: 'SN-2' })], next_cursor: null }
        : { devices: [device()], next_cursor: 'd1' },
    );
    renderPage();

    expect(await screen.findByText(/more available/i)).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: /load more/i }));

    expect(await screen.findByText('SN-2')).toBeTruthy();
    await waitFor(() => expect(screen.queryByRole('button', { name: /load more/i })).toBeNull());
  });
});

describe('the last-seen column', () => {
  it('reads as a duration', () => {
    const now = Date.now();
    expect(ago(new Date(now - 30_000).toISOString())).toBe('30s ago');
    expect(ago(new Date(now - 5 * 60_000).toISOString())).toBe('5m ago');
    expect(ago(new Date(now - 3 * 3_600_000).toISOString())).toBe('3h ago');
    expect(ago(new Date(now - 2 * 86_400_000).toISOString())).toBe('2d ago');
  });
});
