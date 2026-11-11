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

export function getToken(): string | null {
  return token;
}

export function setToken(next: string | null) {
  token = next;
  if (next) localStorage.setItem(STORAGE_KEY, next);
  else localStorage.removeItem(STORAGE_KEY);
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
