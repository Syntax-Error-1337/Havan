import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { TeamPage } from './TeamPage';
import type { Team } from '../../entities/team';

const team: Team = {
  members: [
    {
      user_id: 'u-owner',
      email: 'owner@acme.test',
      display_name: 'Owner',
      role: 'owner',
      last_login_at: '2026-11-10T09:00:00Z',
      sso_bound: true,
    },
    {
      user_id: 'u-dev',
      email: 'dev@acme.test',
      display_name: 'Dev',
      role: 'developer',
      last_login_at: null,
      sso_bound: false,
    },
  ],
  invites: [
    {
      id: 'i-1',
      email: 'pending@acme.test',
      role: 'viewer',
      expires_at: '2026-11-20T00:00:00Z',
      expired: false,
    },
  ],
};

function stubFetch() {
  const calls: { url: string; method: string; body?: string }[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = init?.method ?? 'GET';
      calls.push({ url, method, body: init?.body as string });

      if (method === 'POST') {
        return new Response(JSON.stringify({ token: 'invite-token-123' }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }
      return new Response(JSON.stringify(team), {
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
        <TeamPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('TeamPage', () => {
  it('lists members, their roles and who is pending', async () => {
    stubFetch();
    renderPage();

    expect(await screen.findByText('owner@acme.test')).toBeDefined();
    expect(screen.getByText('dev@acme.test')).toBeDefined();
    expect(screen.getByText('pending@acme.test')).toBeDefined();
    expect(screen.getByText('never')).toBeDefined();
  });

  it('shows the invitation token once, because it cannot be retrieved again', async () => {
    const calls = stubFetch();
    renderPage();

    await screen.findByText('owner@acme.test');

    await userEvent.type(
      screen.getByLabelText('Email to invite'),
      'new@acme.test',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Create invitation' }));

    expect(await screen.findByTestId('invite-token')).toBeDefined();
    expect(screen.getByTestId('invite-token').textContent).toContain('invite-token-123');

    const posted = calls.find((c) => c.method === 'POST');
    expect(posted?.url).toContain('/vendor/team');
    expect(posted?.body).toContain('new@acme.test');
  });
});
