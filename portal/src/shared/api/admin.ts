/**
 * The operator's client.
 *
 * Separate from the vendor one on purpose, and not only because of the header: an operator token
 * sitting in the same store as a vendor token would be one XSS away from being used by the wrong
 * screen, and the two doors must never be able to borrow each other's credentials.
 */

const ADMIN_KEY = 'haven.admin.token';

export class AdminError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
    readonly details?: Record<string, string>,
  ) {
    super(message);
    this.name = 'AdminError';
  }
}

export function getAdminToken(): string | null {
  return localStorage.getItem(ADMIN_KEY);
}

export function setAdminToken(token: string | null) {
  if (token) localStorage.setItem(ADMIN_KEY, token);
  else localStorage.removeItem(ADMIN_KEY);
}

const BASE = import.meta.env.VITE_API_URL ?? '/v1';

export async function adminRequest<T>(path: string, init: RequestInit = {}): Promise<T> {
  const token = getAdminToken();

  const res = await fetch(`${BASE}${path}`, {
    ...init,
    headers: {
      accept: 'application/json',
      ...(init.body ? { 'content-type': 'application/json' } : {}),
      // A token is only sent when there is one: on a local machine the ops door accepts loopback
      // without it, and sending an empty header would turn that into a refusal.
      ...(token ? { 'x-ops-token': token } : {}),
      ...((init.headers as Record<string, string>) ?? {}),
    },
  });

  if (res.status === 204) return undefined as T;

  const text = await res.text();
  const body = text ? safeParse(text) : undefined;

  if (!res.ok) {
    const err = (body as { error?: { code: string; message: string; details?: Record<string, string> } })
      ?.error;

    // A wrong token must not leave the console believing it is signed in.
    if (res.status === 401 || res.status === 403) setAdminToken(null);

    throw new AdminError(
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
