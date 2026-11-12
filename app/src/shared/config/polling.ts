/**
 * How often screens re-poll, when they need to at all.
 *
 * Polling exists for the two things a websocket does not carry: the unread
 * notification count and the alert list. Under test it is off.
 *
 * That is not a test-hack: a pending 30-second timer is a live handle, and a
 * handle is exactly what makes jest hang and a reviewer reach for `--forceExit`.
 * A test that wants a second poll can call `refetch()` explicitly, which is what
 * the assertion was really about anyway.
 */
export const POLL_MS: number | false = process.env.NODE_ENV === 'test' ? false : 30_000;
