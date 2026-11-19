import { ApiError } from './client';
import { describeFailure } from './errors';

/**
 * What a customer reads when a screen does not load.
 *
 * The app had the mirror image of the portal's problem: the portal printed raw JavaScript (`TypeError: Failed to fetch`),
 * and the app printed one fixed sentence blaming the network — even when the platform had answered perfectly well and said
 * something specific. Both mislead, in opposite directions.
 */
describe('describeFailure', () => {
  it('repeats the platform’s own sentence, because it is written for a person', () => {
    const error = new ApiError(409, 'conflict', 'that home already has a room with that name');
    expect(describeFailure(error).body).toBe('that home already has a room with that name');
  });

  it('does not blame the network for an answer that came back', () => {
    const error = new ApiError(403, 'forbidden', 'Request failed (403)');
    const { body, permanent } = describeFailure(error);
    expect(body).toMatch(/do not have access/i);
    expect(body).not.toMatch(/could not reach/i);
    // A refusal the platform will repeat: offering Try again would teach the customer to press it forever.
    expect(permanent).toBe(true);
  });

  it('treats a deleted thing as gone rather than as a connection problem', () => {
    const { body, permanent } = describeFailure(new ApiError(404, 'not_found', 'Request failed (404)'));
    expect(body).toMatch(/removed/i);
    expect(permanent).toBe(true);
  });

  it('says the session ended when it did', () => {
    const { body } = describeFailure(new ApiError(401, 'unauthorized', 'Request failed (401)'));
    expect(body).toMatch(/sign in again/i);
  });

  it('blames the connection only when nothing came back', () => {
    // What a dropped connection throws in a browser and in React Native.
    for (const thrown of [new TypeError('Failed to fetch'), new Error('Network request failed')]) {
      const { body, permanent } = describeFailure(thrown);
      expect(body).toMatch(/could not reach the hub/i);
      expect(body).toMatch(/nothing has changed/i);
      expect(permanent).toBe(false);
    }
  });

  it('reads the client’s own offline error as the connection, not as the platform’s words', () => {
    // `client.ts` throws ApiError(0, 'network', 'You appear to be offline.') when fetch itself fails.
    const { body, permanent } = describeFailure(new ApiError(0, 'network', 'You appear to be offline.'));
    expect(body).toMatch(/could not reach the hub/i);
    expect(body).toMatch(/nothing has changed/i);
    expect(permanent).toBe(false);
  });

  it('falls back to the connection for anything that is not an answer', () => {
    expect(describeFailure(undefined).body).toMatch(/could not reach the hub/i);
    expect(describeFailure('boom').body).toMatch(/could not reach the hub/i);
  });
});
