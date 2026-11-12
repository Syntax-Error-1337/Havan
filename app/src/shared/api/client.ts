/**
 * The one HTTP client. Screens never call `fetch`; they call a query hook that
 * calls this. When the OpenAPI client lands, only this file changes.
 */

import Constants from 'expo-constants';

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

/**
 * Where the API is.
 *
 * `localhost` is right on a simulator and wrong on a phone: a phone loading the bundle
 * from `exp://192.168.0.49:8081` has its own localhost, so every request goes nowhere and
 * the app looks broken. Metro knows the address it served from, so the API is assumed to
 * be on that same host — true of every dev setup where the two run side by side.
 *
 * `EXPO_PUBLIC_API_URL` still wins: a deployment, or an API somewhere else, says so.
 */
function apiBase(): string {
  const explicit = process.env.EXPO_PUBLIC_API_URL;
  if (explicit) return explicit;

  // "192.168.0.49:8081" on a LAN dev server, "localhost:8081" on a simulator: take the
  // host, swap the port.
  const hostUri: string | undefined =
    Constants.expoConfig?.hostUri ??
    (Constants.expoGoConfig as { debuggerHost?: string } | undefined)?.debuggerHost;
  const host = hostUri?.split(':')[0];
  return host ? `http://${host}:8080/v1` : 'http://localhost:8080/v1';
}

export const BASE = apiBase();

let accessToken: string | null = null;
let refreshHandler: (() => Promise<string | null>) | null = null;

export function setAccessToken(token: string | null) {
  accessToken = token;
}

/** Installed by the session slice; kept out of here so this file has no policy. */
export function setRefreshHandler(fn: () => Promise<string | null>) {
  refreshHandler = fn;
}

export async function request<T>(
  path: string,
  init: RequestInit = {},
  retried = false,
): Promise<T> {
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

  // An expired access token is a retry, not a logout. One attempt only: a
  // refresh loop would hammer the server on a revoked family.
  if (res.status === 401 && !retried && refreshHandler) {
    const fresh = await refreshHandler();
    if (fresh) return request<T>(path, init, true);
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
