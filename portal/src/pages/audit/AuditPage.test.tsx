import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { AuditPage, when, who } from './AuditPage';
import type { AuditEntry, AuditPage as Page } from '../../entities/audit';

const entry = (over: Partial<AuditEntry> = {}): AuditEntry => ({
  id: 'a1',
  ts: '2026-11-12T09:17:17Z',
  actor_kind: 'user',
  actor_id: 'aaaaaaaa-1431-7252-b709-b3856dc1a575',
  action: 'api_key.revoked',
  target_kind: 'api_key',
  target_id: 'bbbbbbbb-1431-7252-b709-b3856dc1a576',
  request_id: 'req-1',
  ip: '127.0.0.1',
  detail: {},
  ...over,
});

function stubFetch(body: Page) {
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
        <AuditPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('audit log', () => {
  it('shows what happened, to what, and from where', async () => {
    stubFetch({ entries: [entry()], next_cursor: null });
    renderPage();

    expect(await screen.findByText('api_key.revoked')).toBeTruthy();
    expect(screen.getByText('127.0.0.1')).toBeTruthy();
    // The target is shortened to a prefix: a full uuid in a table is unreadable.
    expect(screen.getByText('bbbbbbbb', { exact: false })).toBeTruthy();
  });

  it('says it is append-only, so nobody looks for a delete button', async () => {
    stubFetch({ entries: [entry()], next_cursor: null });
    renderPage();
    expect(await screen.findByText(/Append-only/i)).toBeTruthy();
  });

  it('names the actor kinds in words', async () => {
    expect(who(entry())).toMatch(/^user /);
    expect(who(entry({ actor_kind: 'api_key', actor_id: null }))).toBe('an API key');
    expect(who(entry({ actor_kind: 'system', actor_id: null }))).toBe('automatic');
  });

  it('prints details that fit, and ignores structured noise', async () => {
    stubFetch({
      entries: [entry({ detail: { name: 'factory line 2', nested: { a: 1 } } })],
      next_cursor: null,
    });
    renderPage();
    expect(await screen.findByText(/name=factory line 2/)).toBeTruthy();
    expect(screen.queryByText(/nested=/)).toBeNull();
  });

  it('shows an empty state that names what would appear', async () => {
    stubFetch({ entries: [], next_cursor: null });
    renderPage();
    expect(await screen.findByText(/Nothing recorded yet/i)).toBeTruthy();
  });

  it('formats a timestamp to the minute', () => {
    // Local time on purpose: an incident is placed against a person's day.
    expect(when('2026-11-12T09:17:17Z')).toMatch(/^2026-11-12 \d{2}:\d{2}$/);
  });
});
