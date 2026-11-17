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

    // The affordance is the button now, not the words "more available": the count says how many there are
    // and the button says what the next press does.
    await userEvent.click(await screen.findByRole('button', { name: /load more/i }));

    expect(await screen.findByText('SN-2')).toBeTruthy();
    await waitFor(() => expect(screen.queryByRole('button', { name: /load more/i })).toBeNull());
  });
});

describe('a fleet bigger than a page', () => {
  // A stub that answers by route, because the page now asks two questions: the devices, and how many
  // there are. The old stub answered every URL with a device page, which hid a missing total.
  function stubByRoute() {
    const urls: string[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        urls.push(url);
        const body = url.includes('/vendor/analytics')
          ? {
              devices: { total: 1909, online: 0, bound: 1909, unbound: 0 },
              new_devices: { last_7d: 0, last_30d: 0 },
              by_product: [],
              firmware: [],
            }
          : {
              devices: Array.from({ length: 200 }, (_, i) => device({ id: `d${i}`, serial: `SN-${i}` })),
              next_cursor: 'cursor-2',
            };
        return new Response(JSON.stringify(body), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }),
    );
    return urls;
  }

  it('asks for the largest page the server allows', async () => {
    // Fifty was the default and the reason a 1 900-device fleet took thirty-eight presses.
    const urls = stubByRoute();
    renderPage();
    await screen.findByText('SN-0');
    expect(urls.some((u) => u.includes('/vendor/devices') && u.includes('limit=200'))).toBe(true);
  });

  it('says how many there are, and loads them in one press', async () => {
    stubByRoute();
    renderPage();

    // "200 devices shown · more available" never told a vendor whether the rest was five or five thousand.
    expect(await screen.findByText('200 of 1,909 devices shown')).toBeTruthy();
    expect(screen.getByRole('button', { name: /load all 1,909/i })).toBeTruthy();
    expect(screen.getByRole('button', { name: /load more/i })).toBeTruthy();
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

  it('puts the device table in a scrollable, named region', async () => {
    // The behaviour lives in CSS and cannot be measured here, but the wiring can: if a refactor
    // drops TableFrame the page silently returns to a table that clips on a phone, and no other
    // test would notice.
    stubFetch(() => ({ devices: [device()], next_cursor: null } as unknown as Page));
    renderPage();

    expect(await screen.findByRole('region', { name: 'Fleet devices' })).toContainElement(
      screen.getByRole('table'),
    );
  });
});
