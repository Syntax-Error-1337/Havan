import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { LoginPage } from './LoginPage';
import { SessionProvider } from '../../app-shell/session';

function stubFetch(status: number, body: unknown) {
  const calls: { url: string; method: string; body?: string }[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      calls.push({ url: String(input), method: init?.method ?? 'GET', body: init?.body as string });
      return new Response(JSON.stringify(body), {
        status,
        headers: { 'content-type': 'application/json' },
      });
    }),
  );
  return calls;
}

function renderLogin() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <SessionProvider>
        <MemoryRouter initialEntries={['/login']}>
          <Routes>
            <Route path="/login" element={<LoginPage />} />
            <Route path="/" element={<p>products screen</p>} />
          </Routes>
        </MemoryRouter>
      </SessionProvider>
    </QueryClientProvider>,
  );
}

describe('vendor login', () => {
  it('sends the vendor id with the credentials', async () => {
    // The same email can exist at two vendors. The vendor is part of the
    // credential, not a lookup detail — without it the login is ambiguous.
    const calls = stubFetch(200, { access: 'a', refresh: 'r' });
    renderLogin();

    await userEvent.clear(screen.getByLabelText('Vendor ID'));
    await userEvent.type(screen.getByLabelText('Vendor ID'), '22222222-2222-7222-8222-222222222222');
    await userEvent.type(screen.getByLabelText('Password'), 'secret12345');
    await userEvent.click(screen.getByRole('button', { name: 'Sign in' }));

    await waitFor(() => expect(calls).toHaveLength(1));
    expect(calls[0]!.url).toContain('/vendor/auth/login');
    expect(JSON.parse(calls[0]!.body ?? '{}')).toEqual({
      vendor_id: '22222222-2222-7222-8222-222222222222',
      email: 'admin@acme.test',
      password: 'secret12345',
    });
  });

  it('lands on the products screen after a successful sign-in', async () => {
    stubFetch(200, { access: 'a', refresh: 'r' });
    renderLogin();

    await userEvent.type(screen.getByLabelText('Password'), 'secret12345');
    await userEvent.click(screen.getByRole('button', { name: 'Sign in' }));

    expect(await screen.findByText('products screen')).toBeInTheDocument();
  });

  it('shows the server’s refusal rather than a blank form', async () => {
    stubFetch(401, { error: { code: 'bad_credentials', message: 'Vendor, email or password is wrong.' } });
    renderLogin();

    await userEvent.type(screen.getByLabelText('Password'), 'nope');
    await userEvent.click(screen.getByRole('button', { name: 'Sign in' }));

    expect(await screen.findByText('Vendor, email or password is wrong.')).toBeInTheDocument();
    expect(screen.queryByText('products screen')).not.toBeInTheDocument();
  });
});
