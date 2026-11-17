# ADR-006 — The theme is cached per process, and that is a choice with a cost

**Status:** accepted

## The problem

`GET /v1/theme` is the busiest read on the platform that nobody thinks about. Every install fetches
it on every cold start, because the sign-in screen has to know the operator's product name, initials,
accent and appearance *before* a session exists. The row it answers from changes when one operator
presses Save — a handful of times in the life of a deployment.

Serving that from Postgres means a million launches a second, at the top of a marketing campaign or a
firmware recall, is a million single-row reads for an answer that has not moved. The HTTP cache already
tells the phone; nothing told the database.

## Decision

**Keep the answer in the API process for 30 seconds, and let a write prime it.**

- `PgTheme` holds `(theme, loaded_at)` behind a mutex and returns it while it is younger than
  `CACHE_TTL` (30 s). Past that it reads the row and replaces the entry.
- `save()` writes and then **primes** the cache with what it wrote, rather than dropping it. The
  operator's own next render must never be the brand they just replaced — the one person guaranteed
  to look immediately is the one who pressed Save.
- `haven_theme_cache_hits_total` is exported in `/metrics`: a cache nobody can see is a cache nobody
  knows is working. Live measurement: three reads, two hits.

## What this costs, stated plainly

**Propagation is per replica.** With N API instances, an operator's change reaches clients through the
instance that served the PUT at once, and through the others within one TTL. Since the public response
also carries `cache-control: max-age=60` and a weak ETag, the customer-visible window is already up to
a minute; the cache does not make that materially worse, and it does not pretend to be instant.

**A wall clock, not a version.** There is no invalidation bus. A replica cannot be told "the theme
changed". That is the trade for not putting Redis in the path of the single most-served endpoint on the
platform — the one endpoint that must answer when everything else is down, including the sign-in screen
of an operator who is trying to find out why.

**Staleness has a floor of one TTL.** A theme change is visible in at most 30 s on replicas that did not
serve it. For a brand colour this is invisible. It would not be acceptable for entitlement data, which
is why this decision is scoped to the theme and not generalised into a cache layer.

## Rejected alternatives

- **Cache in Redis.** One more dependency in the path of the endpoint that must survive Redis being
  down. The chaos drill stops Valkey deliberately; the sign-in screen must still render.
- **A version column and a conditional read.** Cheaper on bytes, not on round trips: you still ask the
  database, which is the thing being avoided.
- **No cache, rely on the CDN.** The API is reached directly by the app in dev, and the platform is
  sold to operators who may run it behind no CDN at all. The process cache is the only layer we
  control in every deployment.
- **Longer TTL.** An operator changing their brand and seeing it in two minutes concludes the button
  is broken. Thirty seconds is short enough to feel immediate after a reload and long enough to absorb
  a launch spike.

## Consequences

- A theme change is live everywhere within 30 s without a deploy, a restart, or a cache to flush.
- The public endpoint answers `304` to a client that already has the brand, and a cache hit to the
  server that does not need to read.
- The cache is a property of the repository, not of the handler, so every caller (public read, ops
  read, tests) shares the same view.
- If a future requirement needs instant global propagation — per-vendor branding, for instance —
  this ADR is the thing to revisit, and the answer will be an invalidation channel, not a longer TTL.
