import { ApiError } from './client';

/**
 * What an operator reads when a screen does not load.
 *
 * Every page printed `error.message`, which is the right thing when the server answered — its messages are written for a
 * person ("that invitation has already been used") — and the wrong thing when it did not: a dropped connection surfaces as
 * `TypeError: Failed to fetch`, and a vendor reading that learns nothing about whether their click was applied.
 *
 * The distinction this function draws is the one that matters: *did the platform answer, or did the question not arrive?*
 * "Nothing was changed" is only true of a request that never reached the server, so it is only said about those.
 */
export function describeError(error: unknown): string {
  if (error instanceof ApiError) {
    // The server's own sentence, unless it is one of its placeholders.
    const fromServer = error.message?.trim();
    if (fromServer && !/^request failed \(\d+\)$/i.test(fromServer)) {
      return fromServer;
    }
    switch (Math.floor(error.status / 100)) {
      case 4:
        return error.status === 404
          ? 'That does not exist any more. It may have been removed by somebody else.'
          : 'The platform refused that request.';
      default:
        return 'The platform could not answer. Nothing was changed — try again in a moment.';
    }
  }

  // No status at all: the request never reached the server, or the answer never came back.
  if (error instanceof Error) {
    return 'The portal could not reach the platform. Check your connection — nothing was changed.';
  }

  return 'Something went wrong. Try again, and tell support if it keeps happening.';
}

/** The error's code, for the `title` attribute: useful when a screenshot goes to support, invisible otherwise. */
export function errorCode(error: unknown): string | undefined {
  return error instanceof ApiError ? error.code : undefined;
}
