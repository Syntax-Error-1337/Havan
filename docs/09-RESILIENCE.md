# What happens when a dependency dies

The drill: `make chaos` (or `make chaos WHAT=mqtt`). It stops a local container, measures
what the platform does, starts it again, and asserts the answers below. 17 assertions,
all passing.

The point is not that things fail — with a dependency gone they must. The point is *how*:
a refusal a client can act on, never a `500` with a driver message, never a `200` that
loses a write, and recovery without restarting the API.

| Dependency gone | `/healthz` | Reads | Commands | Recovery |
|---|---|---|---|---|
| Valkey | `redis: false`, `503` | work | work, `202` | automatic |
| VerneMQ | `mqtt: false`, `503` | work | **`503`, nothing sent** | automatic |
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

## What is not covered

- **Durable redelivery.** A command refused during an outage is not retried later; the
  customer's client is told to retry. A redelivery queue would make `202` truthful through
  a broker blip, and needs a delivery-attempt column before it can exist.
- **Partial failure.** This kills whole containers. A broker that accepts connections and
  drops messages, or a database that answers slowly rather than not at all, is a different
  drill.
- **Multiple replicas.** The drill runs one API process. Whether two replicas behind a load
  balancer disagree about health is untested here.
- **Real hardware.** No device is involved, so "the command reached the device" is out of
  scope by construction.
