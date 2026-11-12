import { QueryClient } from '@tanstack/react-query';

/**
 * The query client every test uses, and the reason `--forceExit` is not needed.
 *
 * React Query schedules a garbage-collection timer per query — five minutes by
 * default — and those timers hold the Node event loop open after the last
 * assertion. Setting `gcTime: 0` fixes the timers but breaks a test that seeds the
 * cache without mounting a component: the query is removed before it can be read.
 *
 * So the timers stay as they are in production, and every client built here is
 * tracked and cleared after each test. `clear()` drops the queries and their
 * timers, which is what the process was waiting for.
 */
const created: QueryClient[] = [];

export function testQueryClient(): QueryClient {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  created.push(client);
  return client;
}

/** Called from `src/test/setup.ts` after every test. */
export function clearTestQueryClients(): void {
  for (const client of created.splice(0)) {
    client.clear();
  }
}
