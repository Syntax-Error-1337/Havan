import { ApiError } from '../../../shared/api/client';

/**
 * What to tell a person when a command did not go through.
 *
 * The toggle has already flipped back by the time this is shown, so "something went
 * wrong" leaves them tapping a switch that appears broken. Each case below names the
 * thing that is actually true — and, where the platform is already doing something about
 * it, says so, because a confident message is also a promise about what happens next.
 */
export function commandNotice(error: unknown): string {
  if (!(error instanceof ApiError)) return 'Could not reach the hub. Try again.';

  switch (error.code) {
    case 'unavailable':
      // The broker is down. The server refuses rather than pretending, and a worker
      // redelivers for two minutes — so the honest message is "not sent yet", not "failed".
      return 'Hub unreachable — not sent. Trying again for 2 minutes.';
    case 'rate_limited':
      return 'Too many attempts. Wait a minute, then try again.';
    case 'forbidden':
      return 'You no longer have access to this device.';
    case 'not_found':
      return 'This device is no longer in your home.';
    default:
      return error.message;
  }
}

/** Whether the platform is still trying, as opposed to having given up. */
export function isRetrying(error: unknown): boolean {
  return error instanceof ApiError && error.code === 'unavailable';
}
