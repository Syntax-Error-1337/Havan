# ADR-003 — Using VerneMQ properly

**Status:** accepted, partly implemented
**Context:** a single-node VerneMQ 2.0.1 broker in front of the platform's devices (TLS, mutual or
password), the API and the workers. The broker was configured by hand, and some of that
configuration was written from experiments that were misread — most importantly a comment claiming
`auth_on_publish` is never called, which is false and which hid a real risk.

This ADR records what the broker does for us, what we were building ourselves, and what changed.

## Measured, not assumed

| Question | Measurement | Answer |
|---|---|---|
| Are the publish/subscribe hooks registered? | `vmq-admin webhooks show` | All three: `auth_on_register`, `auth_on_publish`, `auth_on_subscribe` |
| Is `auth_on_publish` actually called? | one QoS 1 publish, then `haven_http_requests_total{route="/v1/internal/mqtt/authorize"}` | **5 → 6.** It is called, once per publish |
| Is the topic ACL enforced? | the conf recorded a test with the endpoint on a dead port: publishes still succeeded | The hook is called, then **fails open** when we do not answer 200 — the ACL is not enforced during an API outage |

The old comment in `deploy/vernemq.conf` concluded "VerneMQ 2.0.1 does not consult
`auth_on_publish`" from the dead-port test. The hook was consulted; what the test actually showed is
that a failed webhook call is *ignored*. That distinction matters: one is a broker limitation, the
other is a security hole, and only the second one needs fixing.

## What VerneMQ already provides, and what we hand-rolled anyway

| Capability | Broker feature | What we do today | Verdict |
|---|---|---|---|
| Fan-out to several workers without duplicate processing | shared subscriptions `$share/<group>/<topic>` | already used — `topics::INGEST_SHARED_FILTER` | good, keep |
| Per-device topic authorisation | `vmq_acl` pattern file with `%u`/`%c`, or `vmq_diversity` against Postgres | `plugins.vmq_acl = off`; the API decides each publish over HTTP | **fix: add the ACL as a second authority** |
| Device auth | `vmq_diversity` + Postgres, or `vmq_passwd` | webhook `auth_on_register` with the device secret | keep — it also re-reads live state |
| Auth call latency | webhook response caching (`cache-control: max-age=N` on the reply) | every connect/subscribe/publish calls the API over HTTP | **fix: cache the register decision briefly** |
| Broker metrics | HTTP listener: `/metrics` (Prometheus), `/status`, `/health`, management API on 8888 | we removed the generated listener and never replaced it; only our own `/metrics` exists | **fix: declare it** |
| Presence events | `on_client_offline`, `on_client_wakeup`, `on_client_gone` hooks | inferred from heartbeats and `last_seen_at` | consider — would make presence event-driven |
| Offline message queue | `max_offline_messages`, `max_online_messages`, `retry_interval` | our devices use clean sessions and we built `command_delivery` retries | keep ours; the queue cannot help a clean session |
| TLS client identity | `listener.ssl.*` with `require_certificate` | mutual TLS on 8883, password on 8884 for the ESP8266's heap | keep |
| Retained messages | `vmq-admin retained show` | shadows live in Postgres | keep ours |
| Cluster | `listener.vmq.clustering`, swc metadata | single node, `no peer available` in the log | silence the cluster listener on a single node |

## Decisions

1. **The API stays the authority; the broker gets a floor.** The webhook knows things a static file
   cannot — the device's home, its current secret, whether it was released. But it fails open, so a
   file ACL is added underneath it that says: the service identity may use everything, and a device
   may only use its own prefix. When the API is down, devices keep their own topics and cannot
   reach anybody else's. Two independent authorities, and the weaker one still bounds the damage.

2. **The HTTP listener is declared explicitly.** The image generates it at every start with a
   hard-coded address, and a copy of that generated block is what broke port 1883 (see the broker
   commit). Declaring `listener.http.default` in our own file keeps it stable and gives us
   `/metrics`, `/status`, `/health` and the management API for free.

3. **Register decisions are cached for a few seconds, publish decisions are not.** A device
   reconnecting every 90 s does not need a fresh HTTP round-trip; a publish is where the payload is
   and where the ACL must be current. The cache is short enough that releasing a device takes effect
   in the time it takes to notice.

## Consequences

- An API outage no longer means "any device may publish anywhere". It means "each device may use its
  own prefix until the API returns", which is recoverable.
- The broker has metrics of its own — queue depth, session counts, dropped messages — that we were
  blind to.
- Two places now describe topic permissions (the file ACL and `domain::mqtt_acl`). They are not
  allowed to disagree; the ACL file is deliberately coarse (a prefix per identity) and the API is
  precise, so the gap is one-dimensional and testable.

## Not adopted, and why

- **`vmq_diversity` with the Postgres driver** for auth: it would remove the HTTP hop, but it reads
  the database with a database credential inside the broker, and our RLS model assumes every read
  goes through a tenant context. The webhook is the only path that can apply the platform's rules
  (`domain::mqtt_acl`) without duplicating them in Lua.
- **Offline queues**: our devices deliberately use clean sessions (the firmware resubscribes on
  CONNACK), so the broker has nothing to queue for them. `command_delivery` already records what was
  refused and retries with a bound.
- **Retained messages as a shadow store**: a retained message is one value per topic with no history
  and no transaction; the shadow is a row with a `version` and an audit trail.
