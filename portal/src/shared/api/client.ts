/**
 * The one HTTP client. Screens never call `fetch`; they call a query hook.
 *
 * The token is kept in memory and mirrored to localStorage. That is acceptable
 * for a desktop admin tool today and is the wrong place for it long term: the
 * fix is an httpOnly refresh cookie, which needs a same-site deployment.
 */

export type ApiErrorBody = {
  error: { code: string; message: string; details?: Record<string, string> };
};

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
    readonly details?: Record<string, string>,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

const BASE = import.meta.env.VITE_API_URL ?? '/v1';
const STORAGE_KEY = 'haven.portal.token';

let token: string | null = localStorage.getItem(STORAGE_KEY);

/**
 * The token as it stands in storage, read fresh.
 *
 * `getToken()` returns a module-level copy taken at import: fine for a request, wrong for a render that has to know who
 * is signed in now — a sign-in later in the session would leave the copy stale until the page reloaded.
 */
export function storedToken(): string | null {
  return localStorage.getItem(STORAGE_KEY);
}

export function getToken(): string | null {
  return token;
}

type Listener = () => void;
const listeners = new Set<Listener>();

/**
 * Told whenever the token changes, including when a 401 clears it.
 *
 * Without this the client could clear the token and the session provider would
 * keep rendering the signed-in shell from React state, so an expired session
 * showed the API's message ("missing bearer token") instead of the login screen.
 */
export function onTokenChange(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function setToken(next: string | null) {
  token = next;
  if (next) localStorage.setItem(STORAGE_KEY, next);
  else localStorage.removeItem(STORAGE_KEY);
  for (const listener of listeners) listener();
}

export async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    ...init,
    headers: {
      accept: 'application/json',
      ...(init.body ? { 'content-type': 'application/json' } : {}),
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      ...((init.headers as Record<string, string>) ?? {}),
    },
  });

  if (res.status === 204) return undefined as T;

  const text = await res.text();
  const body = text ? safeParse(text) : undefined;

  if (!res.ok) {
    const err = (body as ApiErrorBody | undefined)?.error;

    // A vendor token that stopped working means the session is over; the gate
    // in App.tsx reacts to this by sending the user back to login.
    if (res.status === 401) setToken(null);

    throw new ApiError(
      res.status,
      err?.code ?? 'unknown',
      err?.message ?? `Request failed (${res.status})`,
      err?.details,
    );
  }

  return body as T;
}

function safeParse(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

/**
 * Fetch a document that is not JSON — the support bundle — and hand it to the browser.
 *
 * `request` parses JSON, so a screen that needs the raw body would otherwise re-implement the token
 * header, and a second copy of auth is a second place to get it wrong. Saved with the name the
 * server suggests when it sends one.
 */
export async function download(path: string, fallbackName: string): Promise<void> {
  const res = await fetch(`${BASE}${path}`, {
    headers: token ? { authorization: `Bearer ${token}` } : {},
  });
  if (!res.ok) {
    let message = `download failed (${res.status})`;
    try {
      const body = (await res.json()) as ApiErrorBody;
      message = body.error.message;
    } catch {
      /* not JSON; the status is all we have */
    }
    throw new ApiError(res.status, 'download_failed', message);
  }

  const blob = await res.blob();
  const disposition = res.headers.get('content-disposition') ?? '';
  const named = /filename="?([^";]+)"?/.exec(disposition)?.[1];
  const url = URL.createObjectURL(blob);
  const link = document.createElement('a');
  link.href = url;
  link.download = named ?? fallbackName;
  link.click();
  URL.revokeObjectURL(url);
}
