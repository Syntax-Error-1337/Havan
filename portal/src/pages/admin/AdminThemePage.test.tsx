import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';

import { AdminThemePage, contrastOn } from './AdminThemePage';

/**
 * The Appearance page, as an operator would work it.
 *
 * The properties that matter are the ones a preview can get wrong on its own: a colour is UNSAVED
 * until the button is pressed, the save carries what the form shows, and the preview's text colour is
 * the same rule the server applies — not "assume white".
 */

const saved = {
  product_name: 'Haven',
  logo_text: 'HV',
  accent: '#0A84FF',
  accent_secondary: '#8B5CF6',
  mode: 'dark' as const,
  accent_on: '#000000',
  updated_at: '2026-11-17T04:00:00Z',
  updated_by: 'ops',
};

const puts: unknown[] = [];

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <AdminThemePage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  puts.length = 0;
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init?: RequestInit) => {
      if (String(url).includes('/ops/theme/history')) {
        return new Response(JSON.stringify({ changes: [] }), { status: 200 });
      }
      if (init?.method === 'PUT') {
        puts.push(JSON.parse(String(init.body)));
        return new Response(JSON.stringify({ ...saved, ...JSON.parse(String(init.body)) }), {
          status: 200,
        });
      }
      return new Response(JSON.stringify(saved), { status: 200 });
    }),
  );
});

describe('the operator appearance page', () => {
  it('shows the saved brand and does not save until asked', async () => {
    renderPage();

    const accent = await screen.findByTestId('theme-accent');
    expect((accent as HTMLInputElement).value).toBe('#0A84FF');

    // Nothing is dirty yet, so Save is unavailable: a colour that applies on a keystroke is a colour
    // two operators see differently while one of them is still typing.
    expect(screen.getByTestId('theme-save')).toBeDisabled();

    await userEvent.clear(accent);
    await userEvent.type(accent, '#14B8A6');

    expect(screen.getByTestId('theme-dirty')).toBeInTheDocument();
    expect(puts).toHaveLength(0);

    await userEvent.click(screen.getByTestId('theme-save'));

    await waitFor(() => expect(puts).toHaveLength(1));
    expect(puts[0]).toMatchObject({ accent: '#14B8A6', product_name: 'Haven', mode: 'dark' });
    await screen.findByTestId('theme-saved');
  });

  it('sends the mode the operator picked, not the one that was', async () => {
    renderPage();
    await screen.findByTestId('theme-accent');

    await userEvent.click(screen.getByTestId('theme-mode-light'));
    await userEvent.click(screen.getByTestId('theme-save'));

    await waitFor(() => expect(puts).toHaveLength(1));
    expect(puts[0]).toMatchObject({ mode: 'light' });
  });

  it('shows the refusal when the server rejects a colour', async () => {
    renderPage();
    await screen.findByTestId('theme-accent');

    vi.stubGlobal(
      'fetch',
      vi.fn(async (url: string, init?: RequestInit) => {
        if (String(url).includes('/ops/theme/history')) {
          return new Response(JSON.stringify({ changes: [] }), { status: 200 });
        }
        if (init?.method === 'PUT') {
          return new Response(
            JSON.stringify({
              error: { code: 'validation_failed', message: 'nope is not a colour; write it as #RRGGBB' },
            }),
            { status: 422 },
          );
        }
        return new Response(JSON.stringify(saved), { status: 200 });
      }),
    );

    await userEvent.clear(screen.getByTestId('theme-accent'));
    await userEvent.type(screen.getByTestId('theme-accent'), 'nope');
    await userEvent.click(screen.getByTestId('theme-save'));

    // A console that swallows the server's reason is a console that lies about having saved.
    const error = await screen.findByTestId('theme-error');
    expect(error.textContent).toContain('#RRGGBB');
  });
});

describe('text colour on the accent', () => {
  it('follows luminance rather than assuming white', () => {
    expect(contrastOn('#FFFFFF')).toBe('#000000');
    expect(contrastOn('#14B8A6')).toBe('#000000');
    expect(contrastOn('#101820')).toBe('#FFFFFF');
    // Shorthand means the same colour as its long form; a preview that disagreed would be wrong
    // exactly when an operator typed the short version.
    expect(contrastOn('#fff')).toBe(contrastOn('#FFFFFF'));
  });
});
