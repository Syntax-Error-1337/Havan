# ADR-002 — only Apache-2.0/BSD infrastructure in the hosting path

**Status:** accepted · **Date:** 2026-11-11

## Context

Haven is hosted for customers, so every component we run is a component we
*distribute as a service*. Source-available licences (BSL, SSPL, RSALv2, TSL,
AGPL) each restrict that in a way a commercial platform cannot accept without a
paid licence. The first draft of the stack had three such components.

## Decisions

| Component | Was | Now | Licence now | Why |
|---|---|---|---|---|
| MQTT broker | EMQX ≥5.9 (BSL 1.1) | **VerneMQ** | Apache-2.0 | see `ADR-001-broker.md` |
| Cache / presence | Redis 7.4+ (RSALv2/SSPLv1) | **Valkey 8** | BSD-3-Clause | protocol-compatible fork under the Linux Foundation; the `redis` crate speaks it unchanged |
| Time series | TimescaleDB Community (TSL) | **plain Postgres 16 partitions** | PostgreSQL licence | TSL restricts offering it as a hosted service; native range partitioning + BRIN + our own hourly rollups cover the same ground |
| Object storage | MinIO (AGPLv3) | **SeaweedFS** in dev, S3/R2 in prod | Apache-2.0 | `object_store` already abstracts the backend; SeaweedFS exposes the S3 API |

## Consequences

- **Telemetry is our job now.** Timescale gave us compression and continuous
  aggregates. We replace them with: monthly range partitions, `BRIN` on `ts`, a
  retention job that drops whole partitions, and a `telemetry_rollup` table the
  worker maintains hourly. That is ~200 lines of scheduled SQL we control
  instead of a licence we do not.
- **Rough capacity.** Partitioned Postgres with batched `COPY` sustains roughly
  10–15 k rows/s per node; beyond that, the ingest writer shards by device and
  the schema is already a plain table, so a ClickHouse sink (Apache-2.0) is an
  additive change rather than a migration.
- **No extension in CI.** Tests run against stock `postgres:16`, which is also
  what customers get on-prem.
- **A licence gate in CI.** `cargo deny check licenses` plus an npm licence
  allow-list (Apache-2.0, MIT, BSD, ISC, 0BSD) fails the build on a new
  copyleft or source-available dependency. A licence problem found at due
  diligence is a rewrite; found in CI it is a one-line swap.

## Revisit when

- A customer contract explicitly permits TSL/BSL components.
- Ingest passes ~15 k rows/s per Postgres node and ClickHouse becomes cheaper
  than sharding.
