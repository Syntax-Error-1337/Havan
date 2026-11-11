# ADR-001 — the MQTT broker is VerneMQ

**Status:** accepted · **Date:** 2026-11-11 · **Supersedes:** the EMQX mention in
the first draft of `01-ARCHITECTURE.md`

## Context

Haven is sold as a platform, which includes running the broker for customers.
The licence of the broker is therefore a commercial constraint, not a footnote.

| Broker | Licence | Verdict |
|---|---|---|
| **EMQX** ≥ 5.9 | Business Source License 1.1 | **Rejected.** Not OSI-approved; the BSL restricts offering the software as a managed or hosted service, which is exactly what we sell. 5.8 and older are Apache-2.0 but end-of-life. |
| **VerneMQ** | Apache-2.0 | **Chosen.** MQTT 5 and 3.1.1, clustering, TLS/mTLS, shared subscriptions, and a Lua plugin (`vmq_diversity`) that can authenticate and authorise against our own Postgres tables. |
| **Apache BifroMQ** | Apache-2.0 (incubating) | Viable alternative, designed for multi-tenant IoT. Younger; smaller operational track record. |
| **Eclipse Mosquitto** | EPL-2.0 / EDL | Excellent single node; no clustering, so it cannot be the multi-tenant backbone. Fine as the dev broker or an edge gateway. |
| **HiveMQ CE** | Apache-2.0 | No clustering in the community edition. |
| **Apache ActiveMQ Artemis** | Apache-2.0 | Genuine Apache TLP, clustered, MQTT 5 support; heavier and not IoT-tuned. |

## Decision

VerneMQ 2.x, Apache-2.0, one cluster per region, three nodes.

Why VerneMQ over BifroMQ:

1. **Maturity.** Years of production IoT deployments; BifroMQ is still incubating.
2. **Auth plug-in fits our schema.** `vmq_diversity` runs Lua with a Postgres
   driver, so authentication and topic authorisation read `devices` / `mqtt_acl`
   directly — no sidecar authorisation service to build and keep consistent.
3. **Shared subscriptions.** `$share/ingest/...` is what lets `haven-worker`
   scale horizontally; VerneMQ supports it alongside MQTT 5 features we rely on
   (session expiry, reason codes, `receive_maximum`).

## Consequences

- **We stay portable.** Nothing outside `infra::mqtt` names the broker: topic
  construction is `protocol::topics`, publishing is one method. Swapping to
  BifroMQ or an EMQX 5.8 LTS is a `infra` change plus deployment config.
- **Erlang in the ops surface.** A new runtime to hire for. Mitigated by
  containerised upgrades and `vmq-admin` for the handful of operations we need
  (cluster status, session count, disconnect a device).
- **Authorisation lives in SQL.** A device that is deleted must lose its ACL
  immediately; the Lua hook queries on every connect and on an interval, and the
  deletion path also disconnects the session explicitly.
- **Dev uses the same image** with anonymous access enabled, so what CI speaks to
  is the same protocol implementation production runs.

## Not chosen, and why it matters

EMQX is the better-known product and the faster start. It is rejected on
licence alone: a BSL broker cannot be the foundation of a platform we host for
customers. That is a board-level constraint, not a preference, so it is recorded
here rather than rediscovered during due diligence.
