import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { AuditPage, targetLabel, when, who } from './AuditPage';
import type { AuditEntry, AuditPage as Page } from '../../entities/audit';

const entry = (over: Partial<AuditEntry> = {}): AuditEntry => ({
  id: 'a1',
  ts: '2026-11-12T09:17:17Z',
  actor_kind: 'user',
  actor_id: 'aaaaaaaa-1431-7252-b709-b3856dc1a575',
  actor_email: 'admin@acme.test',
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

  it('names the actor kinds in words, and a person by their address', async () => {
    // "Who did this" is the question an audit trail exists to answer; a uuid prefix cannot answer it.
    expect(who(entry())).toBe('admin@acme.test');
    // A member whose account has since gone leaves the id — which is itself worth seeing in a trail.
    expect(who(entry({ actor_email: null }))).toMatch(/^user /);
    expect(who(entry({ actor_kind: 'api_key', actor_id: null }))).toBe('an API key');
    expect(who(entry({ actor_kind: 'system', actor_id: null }))).toBe('automatic');
  });

  it('prints details that fit, and ignores structured noise', async () => {
    stubFetch({
      // `name` becomes the target label, so the detail line is what is left: `role=developer`. Structured noise is
      // still dropped, and a value already shown in the row is not printed twice.
      entries: [entry({ detail: { name: 'factory line 2', role: 'developer', nested: { a: 1 } } })],
      next_cursor: null,
    });
    renderPage();
    // The target column already says "factory line 2", so the detail line carries the rest and drops the rest of it:
    // nested objects are noise on a one-line summary.
    expect(await screen.findByText(/role=developer/)).toBeTruthy();
    expect(screen.queryByText(/name=factory line 2/)).toBeNull();
    expect(screen.queryByText(/nested/)).toBeNull();
    expect(screen.queryByText(/nested=/)).toBeNull();
  });

  it('shows an empty state that names what would appear', async () => {
    stubFetch({ entries: [], next_cursor: null });
    renderPage();
    expect(await screen.findByText(/Nothing recorded yet/i)).toBeTruthy();
  });

  it('formats a timestamp to the second', () => {
    // Local time on purpose: an incident is placed against a person's day. Seconds because minutes could not order
    // the trail — four entries shared one minute, so "newest first" could not be checked against the screen.
    expect(when('2026-11-12T09:17:17Z')).toMatch(/^2026-11-12 \d{2}:\d{2}:17$/);
  });
});

describe('the target, named', () => {
  it('uses the name the action recorded, and falls back to the kind', async () => {
    expect(targetLabel(entry({ action: 'webhook.created', detail: { url: 'https://acme.test/hook' } }))).toBe(
      'https://acme.test/hook',
    );
    expect(targetLabel(entry({ action: 'api_key.created', detail: { name: 'factory line 2' } }))).toBe(
      'factory line 2',
    );
    // Nothing named: the kind and a short id still say which sort of thing changed.
    expect(targetLabel(entry({ target_kind: 'vendor_webhook', detail: {} }))).toMatch(/^vendor webhook /);
  });
});

describe('narrowing the trail', () => {
  it('sends the filters to the server, and says an empty answer is an answer', async () => {
    const urls: string[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (input: RequestInfo | URL) => {
        urls.push(String(input));
        return new Response(JSON.stringify({ entries: [], next_cursor: null }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }),
    );

    renderPage();

    await userEvent.type(await screen.findByTestId('audit-action'), 'api_key.revoked');
    await vi.waitFor(() =>
      expect(urls.some((u) => u.includes('action=api_key.revoked'))).toBe(true),
    );

    // An empty result under a filter must not read as "the trail is empty".
    expect(await screen.findByText(/Nothing matches the action api_key\.revoked/)).toBeTruthy();
    expect(screen.queryByText(/Nothing recorded yet/)).toBeNull();
  });
});
