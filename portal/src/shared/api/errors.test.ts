import { describe, expect, it } from 'vitest';

import { ApiError } from './client';
import { describeError } from './errors';

/**
 * A screen that fails to load is the worst place to print a JavaScript error.
 *
 * The rule these tests hold: the server's own sentence is shown when there is one, because those messages are written for
 * a person; when there is not, the message says what happened to the request — and "nothing was changed" is only claimed
 * about a request that never arrived.
 */
describe('describeError', () => {
  it('repeats what the server said, because it was written for a person', () => {
    const error = new ApiError(409, 'conflict', 'that invitation has already been used');
    expect(describeError(error)).toBe('that invitation has already been used');
  });

  it('replaces the client’s own placeholder with something an operator can act on', () => {
    const error = new ApiError(502, 'unknown', 'Request failed (502)');
    expect(describeError(error)).toBe(
      'The platform could not answer. Nothing was changed — try again in a moment.',
    );
  });

  it('does not claim anything was changed when the request never arrived', () => {
    // What a dropped connection actually throws.
    expect(describeError(new TypeError('Failed to fetch'))).toBe(
      'The portal could not reach the platform. Check your connection — nothing was changed.',
    );
  });

  it('names a missing row instead of a status code', () => {
    expect(describeError(new ApiError(404, 'not_found', 'Request failed (404)'))).toBe(
      'That does not exist any more. It may have been removed by somebody else.',
    );
  });

  it('has something to say about anything thrown', () => {
    expect(describeError(undefined)).toMatch(/something went wrong/i);
    expect(describeError('a string')).toMatch(/something went wrong/i);
  });
});
