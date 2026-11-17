/**
 * How often screens re-poll, when they need to at all.
 *
 * Polling used to exist for the unread count and the alert list, at 30 seconds each — two requests
 * per signed-in app, forever. At a million apps that is 66 000 requests a second spent asking a
 * question the platform already knows the answer to.
 *
 * The worker now pushes an alert over the socket the app already holds (`user:{id}`), so polling is a
 * *safety net* rather than the mechanism: a slow sweep that repairs whatever a missed push left
 * behind. Five minutes, not thirty seconds.
 *
 * Under test it is off. That is not a test-hack: a pending 30-second timer is a live handle, and a
 * handle is exactly what makes jest hang and a reviewer reach for `--forceExit`. A test that wants a
 * second poll can call `refetch()` explicitly, which is what the assertion was really about anyway.
 */
export const POLL_MS: number | false = process.env.NODE_ENV === 'test' ? false : 5 * 60_000;
