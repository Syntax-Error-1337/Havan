import { commandNotice, isRetrying } from './notices';
import { ApiError } from '../../../shared/api/client';

/**
 * The message is the whole feature: the switch has already flipped back, so anything
 * vague is indistinguishable from a switch that does not work.
 */
describe('command notices', () => {
  it('says the hub is unreachable and that retrying is happening', () => {
    const error = new ApiError(
      503,
      'unavailable',
      'the device broker is unavailable; command 01a1 was not sent',
    );
    expect(commandNotice(error)).toMatch(/Hub unreachable/);
    expect(commandNotice(error)).toMatch(/not sent/);
    // The platform redelivers for two minutes; saying so is the difference between
    // "broken" and "wait".
    expect(commandNotice(error)).toMatch(/2 minutes/);
    expect(isRetrying(error)).toBe(true);
  });

  it('does not promise a retry for a failure that will not retry', () => {
    const error = new ApiError(422, 'validation_failed', 'value out of range');
    expect(commandNotice(error)).toBe('value out of range');
    expect(isRetrying(error)).toBe(false);
  });

  it('names access and membership problems rather than echoing the server', () => {
    expect(commandNotice(new ApiError(403, 'forbidden', 'nope'))).toMatch(/no longer have access/);
    expect(commandNotice(new ApiError(404, 'not_found', 'nope'))).toMatch(/no longer in your home/);
    expect(commandNotice(new ApiError(429, 'rate_limited', 'slow down'))).toMatch(/Wait a minute/);
  });

  it('handles a failure that is not an API error at all', () => {
    // A dropped connection surfaces as a plain TypeError from fetch.
    expect(commandNotice(new TypeError('Network request failed'))).toMatch(/Could not reach/);
    expect(commandNotice(undefined)).toMatch(/Could not reach/);
  });
});
