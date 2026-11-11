import { render, fireEvent, waitFor } from '@testing-library/react-native';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import * as SecureStore from 'expo-secure-store';

import { LoginPage } from './LoginPage';
import { ThemeProvider } from '../../app-shell/theme';
import { useSession } from '../../entities/session';

type Call = { url: string; method: string; body?: string };

function stubApi(handler: (url: string) => { status?: number; body: unknown }) {
  const calls: Call[] = [];
  globalThis.fetch = jest.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    calls.push({ url, method: init?.method ?? 'GET', body: init?.body as string });
    const { status = 200, body } = handler(url);
    return {
      ok: status >= 200 && status < 300,
      status,
      text: async () => JSON.stringify(body),
    } as Response;
  }) as unknown as typeof fetch;
  return calls;
}

async function renderLogin() {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  return await render(
    <QueryClientProvider client={client}>
      <ThemeProvider>
        <LoginPage />
      </ThemeProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  useSession.setState({ access: null, homeId: null, refresh: null });
});

describe('login', () => {
  it('keeps the buttons disabled until the form is plausible', async () => {
    const calls = stubApi(() => ({ body: {} }));
    const view = await renderLogin();

    await fireEvent.changeText(view.getByPlaceholderText('Email'), 'not-an-email');
    await fireEvent.press(view.getByText('Sign in'));
    expect(calls).toHaveLength(0);

    // A 10-character floor is a product decision, not a security one: it stops
    // a typo-then-lockout loop before the server ever sees it.
    await fireEvent.changeText(view.getByPlaceholderText('Email'), 'someone@haven.dev');
    await fireEvent.changeText(view.getByPlaceholderText(/^Password/), 'short');
    await fireEvent.press(view.getByText('Sign in'));
    expect(calls).toHaveLength(0);
  });

  it('signs in, then writes the session to the keychain', async () => {
    const calls = stubApi((url) => {
      if (url.endsWith('/auth/login')) {
        return { body: { access: 'a-token', refresh: 'r-token', expires_in: 900 } };
      }
      return { body: { id: 'u1', email: 'someone@haven.dev' } };
    });

    const view = await renderLogin();
    await fireEvent.changeText(view.getByPlaceholderText('Email'), 'someone@haven.dev');
    await fireEvent.changeText(view.getByPlaceholderText(/^Password/), 'correct horse battery');
    console.log('PROBE button disabled', view.getByText('Sign in').parent?.parent?.props?.accessibilityState);
    await fireEvent.press(view.getByText('Sign in'));

    await waitFor(() => expect(useSession.getState().access).toBe('a-token'));

    // The refresh token belongs in the keychain, nowhere else.
    expect(SecureStore.setItemAsync).toHaveBeenCalledWith('haven.refresh', 'r-token');
    expect(JSON.parse(calls[0]!.body ?? '{}')).toEqual({
      email: 'someone@haven.dev',
      password: 'correct horse battery',
    });
  });

  it('shows the server’s refusal in words, not a stack trace', async () => {
    stubApi(() => ({
      status: 401,
      body: { error: { code: 'bad_credentials', message: 'Email or password is incorrect.' } },
    }));

    const view = await renderLogin();
    await fireEvent.changeText(view.getByPlaceholderText('Email'), 'someone@haven.dev');
    await fireEvent.changeText(view.getByPlaceholderText(/^Password/), 'wrong password');
    await fireEvent.press(view.getByText('Sign in'));

    expect(await view.findByText('Email or password is incorrect.')).toBeTruthy();
    expect(useSession.getState().access).toBeNull();
  });

  it('creates an account through the same two fields', async () => {
    const calls = stubApi(() => ({
      body: { access: 'a2', refresh: 'r2', expires_in: 900 },
    }));

    const view = await renderLogin();
    await fireEvent.press(view.getByText('No account yet? Create one'));
    await fireEvent.changeText(view.getByPlaceholderText('Email'), 'new@haven.dev');
    await fireEvent.changeText(view.getByPlaceholderText(/^Password/), 'correct horse battery');
    await fireEvent.press(view.getByText('Create account'));

    await waitFor(() => expect(useSession.getState().access).toBe('a2'));
    expect(calls[0]!.url).toContain('/auth/register');
  });

  it('says plainly that it is offline rather than blaming the password', async () => {
    globalThis.fetch = jest.fn(async () => {
      throw new TypeError('Network request failed');
    }) as unknown as typeof fetch;

    const view = await renderLogin();
    await fireEvent.changeText(view.getByPlaceholderText('Email'), 'someone@haven.dev');
    await fireEvent.changeText(view.getByPlaceholderText(/^Password/), 'correct horse battery');
    await fireEvent.press(view.getByText('Sign in'));

    expect(await view.findByText('You appear to be offline.')).toBeTruthy();
  });
});
