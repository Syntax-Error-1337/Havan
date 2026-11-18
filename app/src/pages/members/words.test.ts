import { inviteWords } from './MembersPage';

/**
 * What a failed invitation says.
 *
 * The server's answer — "account with that email not found" — is a fact, not an instruction. The customer needs the
 * one thing they can do about it, and anything we do not recognise has to pass through untouched rather than being
 * flattened into "something went wrong".
 */

describe('inviteWords', () => {
  it('turns the server’s 404 into the next step', () => {
    expect(inviteWords(new Error('account with that email not found'))).toBe(
      'Nobody in Haven has that email. Check the spelling, or ask them to create an account first.',
    );
  });

  it('passes an unrecognised message through unchanged', () => {
    expect(inviteWords(new Error('you cannot grant a role above your own'))).toBe(
      'you cannot grant a role above your own',
    );
  });

  it('has something to say when the error carries no message', () => {
    expect(inviteWords(new Error(''))).toBe('That person could not be added.');
    expect(inviteWords(null)).toBe('That person could not be added.');
  });
});
