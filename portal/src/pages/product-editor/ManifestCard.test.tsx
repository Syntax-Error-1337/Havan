import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { ManifestCard } from './ManifestCard';
import { parseSerials } from '../../entities/serials';

function stubFetch(handler: (url: string, init?: RequestInit) => unknown) {
  const calls: { url: string; method: string; body?: string }[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      calls.push({ url, method: init?.method ?? 'GET', body: init?.body as string });
      return { ok: true, status: 200, text: async () => JSON.stringify(handler(url, init)) } as Response;
    }),
  );
  return calls;
}

function renderCard() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ManifestCard productId="p1" />
    </QueryClientProvider>,
  );
}

describe('manifest card', () => {
  it('reads a pasted list however it was pasted', () => {
    // A CSV column, a comma-separated line and a spreadsheet paste all arrive.
    expect(parseSerials('SN-1\nSN-2\nSN-3')).toEqual(['SN-1', 'SN-2', 'SN-3']);
    expect(parseSerials('SN-1,SN-2;SN-3')).toEqual(['SN-1', 'SN-2', 'SN-3']);
    expect(parseSerials('"SN-1"\tSN-2\n\n')).toEqual(['SN-1', 'SN-2']);
  });

  it('shows how many serials are still unclaimed, not how many exist', async () => {
    stubFetch(() => ({ total: 10, claimed: 7, open: 3 }));
    renderCard();

    expect(await screen.findByText(/3 of 10/)).toBeInTheDocument();
    expect(screen.getByText('still unclaimed')).toBeInTheDocument();
  });

  it('uploads the parsed serials and reports what happened', async () => {
    const calls = stubFetch((_url, init) =>
      init?.method === 'POST'
        ? { added: 2, already_listed: 1, summary: { total: 3, claimed: 0, open: 3 } }
        : { total: 0, claimed: 0, open: 0 },
    );
    renderCard();

    await userEvent.type(
      await screen.findByPlaceholderText(/SN-0001/),
      'SN-1{Enter}SN-2{Enter}SN-3',
    );
    await userEvent.click(screen.getByRole('button', { name: /Upload/ }));

    await waitFor(() => expect(calls.some((c) => c.method === 'POST')).toBe(true));
    const body = JSON.parse(calls.find((c) => c.method === 'POST')?.body ?? '{}');
    expect(body.serials).toEqual(['SN-1', 'SN-2', 'SN-3']);

    // Saying only "3 added" would hide that one was already there.
    expect(await screen.findByText('2 added, 1 were already listed')).toBeInTheDocument();
  });

  it('will not upload nothing', async () => {
    stubFetch(() => ({ total: 0, claimed: 0, open: 0 }));
    renderCard();

    expect(await screen.findByRole('button', { name: /Upload/ })).toBeDisabled();
  });
});
