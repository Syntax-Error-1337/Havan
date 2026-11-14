# What happens when a dependency dies

The drill: `make chaos` (or `make chaos WHAT=mqtt`). It stops a local container, measures
what the platform does, starts it again, and asserts the answers below. 18 assertions,
all passing.

The point is not that things fail — with a dependency gone they must. The point is *how*:
a refusal a client can act on, never a `500` with a driver message, never a `200` that
loses a write, and recovery without restarting the API.

| Dependency gone | `/healthz` | Reads | Commands | Recovery |
|---|---|---|---|---|
| Valkey | `redis: false`, `503` | work | work, `202` | automatic |
| VerneMQ | `mqtt: false`, `503` | work | **`503`, nothing sent** | automatic, and the command is delivered late |
| Postgres | `db: false`, `503` | **`503`** | `503` | automatic |

Liveness (`/livez`) never touches a dependency: a probe that fails because Postgres is slow
restarts a healthy pod and turns a blip into an outage.

## The four bugs the drill found

None of these were visible from reading the code, and all four were fixed before the table
above was true.

**1. A dead cache hung the request that touched it.** With Valkey stopped, `POST
/auth/login` never answered at all: curl's five-second deadline expired first. `rumqttc`
and `redis` both wait on a connection that will never come up. Fixed with
`ConnectionManagerConfig` timeouts (2 s connect, 2 s response) so every command is
bounded, not only the ones somebody remembered to wrap.

**2. The rate limiter stood between the cache and every login.** It already failed open on
an *error*, but an error is not what a black-holed cache produces — it produces a hang.
The limiter now gives the cache 250 ms and lets the request through. A limiter that cannot
count must not become a limiter that locks people out.

**3. `/healthz` could not report a dependency that had gone away**, because its `ping`s
were unbounded and the endpoint timed out instead of answering `503` with `db: false`.
Both pings are now bounded at 2 s, and the pool's `acquire_timeout` went from 5 s to 2 s —
a healthy database hands out a connection in microseconds, so waiting five just means the
caller waits five and gets an error anyway.

**4. Commands were reported as sent when the broker was down.** The worst of the four:
`rumqttc`'s `publish` only queues the packet, so with VerneMQ stopped it returned `Ok` and
the API answered `202` for a command that no process would ever deliver. Worse, the row
then aged into `timeout`, and the customer was told **the device** did not respond.

Now `Mqtt::publish` refuses when the connection flag is down, the command is settled as
`unreachable` with `error_code = broker_unavailable` (the schema already had the word),
the caller gets `503` with the command id and "was not sent", and
`haven_commands_undelivered_total` counts them. The difference between that counter and
`haven_commands_dispatched_total` is the outage.

## Late delivery

A `503` is only honest if the command is not lost, so the worker redelivers it.

`commands.attempts` counts every delivery attempt. The refusal records one
(`unreachable / broker_unavailable`), and a five-second sweep in the worker picks up
refused commands from the **last two minutes** and republishes them once the broker is
back, marking `dispatched` and incrementing `attempts`.

Bounded in both directions on purpose:

- **two minutes**, because a command delivered ten minutes late turns a light on for no
  reason — the person has moved on and the house acts on a stale intention;
- **only while connected**, tracked by a flag the worker's own event loop sets on `ConnAck`
  and clears on disconnect, because `rumqttc::publish` accepts packets into a dead socket
  and would otherwise let the sweep mark commands delivered that never left the process.

`mark_redelivered` is conditional on the row still being `unreachable`, so two workers
racing over the same row send it once.

Verified live: broker stopped → `503`, row `unreachable attempts=1 broker_unavailable` →
broker started → within one sweep the row is `dispatched attempts=2` and the worker logs
`redelivered`. The drill asserts exactly this, on the specific command that was refused.

## What is not covered

- **Long outages.** Past two minutes a refused command is not retried at all — it stays
  `unreachable` and the customer's client is the retry mechanism. That is a decision, not
  an oversight: an unbounded queue would replay a morning's presses at lunchtime.
- **Partial failure.** This kills whole containers. A broker that accepts connections and
  drops messages, or a database that answers slowly rather than not at all, is a different
  drill.
- ~~Multiple API replicas~~ — **now covered** by `make api-replicas`: two instances report
  `db: false` and `503` together, recover without a restart, and a replica started while the
  broker is down does not claim `mqtt: true`. That drill found the reason a second replica was
  impossible before: both used the client id `haven-api`, so the broker evicted whichever
  connected first, on every reconnect — `/healthz` flapped between ok and degraded, and the
  losing replica dropped mid-publish. The id is now unique per process (or `HAVEN_INSTANCE`),
  and the API's client uses a clean session because it publishes and subscribes to nothing.
- ~~Multiple workers~~ — **now covered** by `make multi-replica`: two workers share the
  subscription, 480 messages land as exactly 960 rows (no duplicates, no loss), and a refused
  command is claimed by one worker and published once (`attempts = 2`, one `redelivered` log
  line). That drill found a real race — redelivery used to publish *then* mark, so two workers
  could both publish one press; it now claims the row first and releases it if the publish
  fails.
- **Real hardware.** No device is involved, so "the command reached the device" is out of
  scope by construction.

## Broker authentication drill

`make broker-auth` claims two devices and asserts what the 8884 listener answers: a device's own
secret is accepted, a wrong secret and an anonymous client are refused, the device may publish
inside `haven/<vendor>/<home>/<device>/…`, and it may neither subscribe to `haven/#` nor publish to
a sibling's topic. It also checks that the refusal came from the platform's ACL rather than from an
unreachable endpoint, because a hook that cannot be reached refuses everything and would pass a
naive test.

The drill exists because these failures are silent. A permissive ACL, a hook that is never
consulted, and a listener that accepts anonymous clients all look like success from the client —
`plugins.vmq_acl` being enabled and answering first was exactly that, and it went unnoticed until a
wildcard subscribe returned SUBACK 0.
