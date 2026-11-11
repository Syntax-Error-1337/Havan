/**
 * The one HTTP client. Screens never call `fetch`; they call a query hook that
 * calls this. When the OpenAPI client lands, only this file changes.
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

  /** True when retrying the same request cannot help. */
  get permanent(): boolean {
    return this.status >= 400 && this.status < 500 && this.status !== 408 && this.status !== 429;
  }
}

const BASE = process.env.EXPO_PUBLIC_API_URL ?? 'http://localhost:8080/v1';

let accessToken: string | null = null;

export function setAccessToken(token: string | null) {
  accessToken = token;
}

export async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers: Record<string, string> = {
    accept: 'application/json',
    ...(init.body ? { 'content-type': 'application/json' } : {}),
    ...(accessToken ? { authorization: `Bearer ${accessToken}` } : {}),
    ...((init.headers as Record<string, string>) ?? {}),
  };

  let res: Response;
  try {
    res = await fetch(`${BASE}${path}`, { ...init, headers });
  } catch (cause) {
    // Offline is a distinct state in a smart-home app, not a generic failure.
    throw new ApiError(0, 'network', 'You appear to be offline.');
  }

  if (res.status === 204) return undefined as T;

  const text = await res.text();
  const body = text ? safeParse(text) : undefined;

  if (!res.ok) {
    const err = (body as ApiErrorBody | undefined)?.error;
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
