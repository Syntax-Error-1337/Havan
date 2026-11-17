# What happens when a dependency dies

The drill: `make chaos` (or `make chaos WHAT=mqtt`). It stops a local container, measures
what the platform does, starts it again, and asserts the answers below. 20 assertions,
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

## Every queue claims before it sends

Three queues deliver out of a table: automations, webhook deliveries, and push notifications. Two of
them claimed their work with a lease; push selected the rows and committed, so two worker replicas
walked the same notifications and one alert buzzed a phone twice.

The lesson is the shape, not the third fix: **reading a queue is not owning it.** A `SELECT` that
decides what to send has to move the row's own clock forward in the same statement — `next_attempt_at`
here — because the transaction that selected it is gone by the time the request leaves the process. A
lease also makes the crash case boring: the row comes back when the lease expires, without anybody
writing a recovery path.

Leases are sized by the batch, not by taste: one pass claims `BATCH` rows and sends them one at a
time with a request timeout, so the lease must outlast `BATCH × timeout` or a worker hands its own
tail to the next one.

The full audit, so the next loop has a precedent to copy:

| Loop | How it claims | Proven by |
|---|---|---|
| automation scheduler | `FOR UPDATE SKIP LOCKED` inside the lease update | `worker/tests/lease.rs` |
| OTA dispatch | `state = 'sent'` inside the selecting statement | `worker/tests/ota_claim.rs` |
| webhook delivery | lease on `next_attempt_at` (5 min) | `api/tests/webhooks.rs` |
| push delivery | lease on `next_attempt_at` (5 min) | `worker/tests/push.rs` |
| certificate expiry | not a claim — the warning insert dedupes per member and bucket | `notify_certificate_expiry` |

Only the last is allowed to skip a claim, and only because the insert itself is the guard: a second
worker's insert finds the row already there and does nothing.

## A webhook is a delivery per endpoint

Vendor webhooks were written as "one row, one event": the outbox inserted a row per subscription,
and then delivery re-joined every row to every active subscription with a matching event. With five
endpoints that produced five deliveries per row sharing one outbox id, `mark_delivered` fired on the
first success, and the other four never heard the event — while which one did depended on join order.
Nothing tested it, because nothing had ever delivered to more than one endpoint.

The row now names its subscription (`0033`), delivery reads that, and two endpoints are proven to
receive the same event independently: verified live with two local receivers, each given its own
delivery id and its own signature.

The claim itself was the second defect. `due()` selected with `FOR UPDATE SKIP LOCKED` and committed
before sending, so the lock was gone by the time the HTTP request left: two worker replicas could
claim the same row and the vendor would receive the event twice. The claim is a **lease** now — the
row's `next_attempt_at` moves forward for five minutes, which is one pass's worst case (20 rows × a
10 s timeout), and a worker that dies mid-send leaves a lease that simply expires. The command
redelivery path had the same bug and was fixed the same way; this one hid behind a queue nobody had
watched with two replicas.

Two further facts came out of the same pass:

- **Two of the three advertised events were never emitted.** `device.claimed` and `firmware.failed`
  were offered in the portal, validated at subscription time, and written by nothing — a vendor who
  chose them heard silence forever. Both are now written where they happen: the claim transaction for
  the first, the give-up transaction for the second.
- **A timestamp in a hand-built document was an array.** `json!` builds through a serializer that
  reports itself as not human-readable, so `OffsetDateTime` took its compact form and a receiver got
  `created_at: [2026,320,18,44,55,...]`. Typed response structs were always fine, which is why only
  the webhook body, the support bundle and the DSAR export were affected — including the export a
  person is legally owed. All three emit RFC 3339 now.

Rolling deploys were handled rather than assumed away: rows queued by an instance that has not been
restarted arrive without a subscription, so the migration recovers the unambiguous ones (a vendor
with exactly one endpoint for that event) and leaves the rest NULL rather than guessing — delivering
to the wrong endpoint is worse than a missed delivery.

## What the drill learned when it was re-run

The chaos drill was written before the operator console, the licence gate and the presence sweep
existed, so it asserted against a platform that has since grown. Re-running it was the point of the
exercise, and it found two things:

- **`/ops/health` was not covered.** With Postgres down the console's health page is the screen
  somebody trusts at 3am; answering `200` with zeros would be worse than refusing. Two assertions now
  prove it returns `503` while the database is gone and `200` once it returns.
- **A failing assertion said only "401".** The read after recovery was now checked with the body
  captured to a file, so a future failure prints the refusal instead of a number to guess from.

The first re-run reported `reads answered 401 after the database came back` while an equivalent
manual sequence returned `200`; after the line was rewritten it passed on every subsequent run and
the 401 has not recurred. That is written down rather than tidied away: an intermittent assertion
nobody can explain is a warning about the harness, and the drill now keeps the evidence.

Current result: **20/20**, run against the live stack.

## Silence is not presence

`online` was only ever written by an incoming report: `UPDATE devices SET online = $2,
last_seen_at = now()`. Nothing cleared it, so a device that died without a will message — a power
cut, a dead radio, a simulator killed at the terminal — read as online for as long as its row
existed. The operator console's health page caught it on its first read: **99 devices online, none
heard from in fifteen minutes, the oldest report four days old.**

The sweep now runs in the worker every minute and marks devices offline after **90 seconds** of
silence: two missed keepalives, long enough that one dropped packet does not flap a device and short
enough that the number means something. Verified live: `online` went 99 → 0 and agreed with
`seen_recently`.

Two figures rather than one, and the difference is the point:

- **online** is what the platform believes — the last shadow said so;
- **heard recently** is what actually arrived in the last fifteen minutes.

When they disagree, something is holding a stale connection. That is a real fault a single flag
would have hidden, and it is why `/ops/health` and `/admin/health` show both.

Not covered: a device on a metered link that reports hourly would be marked offline between reports.
Ninety seconds matches this fleet's keepalive; a different fleet needs a different window, which is
why the sweep takes it as an argument.

### The sweep has to tell somebody

Fixing the row was half the job. `online` is what the database believes, but the app believes
whatever last arrived on its socket — so a device aged out by the sweep stayed painted green in every
open app until somebody pulled to refresh. The sweep now returns **who** it aged out, not a count, and
publishes one `device.offline` per device on `home:{home_id}`; ingest builds the same frame through
the same function, so the two paths cannot describe presence differently.

A count could not be published, which is why the return type changed rather than gaining a second
"which ones" query — the sweep already knows, and asking again is a second chance to disagree.

Verified live, not asserted: a subscriber on the real channel received

```json
{"device_id":"6d44da90-…","home_id":"01a1e58e-…","t":"device.offline","ts":1794853360}
```

from the running worker's own sweep, and `worker/tests/presence.rs` proves the row, the return value
and the announcement — subscribing *before* the sweep, because Redis has no replay.

## A disk that fills at three in the morning

`telemetry` is partitioned by month, and 0003 created twenty-four partitions up front with a DEFAULT
partition for readings whose clock disagrees with the calendar. Both halves of that sentence were a
promise nobody was keeping:

- **Nothing created a partition after the first twenty-four.** Around November 2028 every reading would
  land in `telemetry_overflow`: still correct, still queryable, and every query plan suddenly scanning a
  table that grows forever instead of a month. A performance cliff with a two-year fuse.
- **Nothing ever dropped one.** At a million devices reporting once a minute that is 1.4 billion rows a
  day, and the first symptom is a full disk and a database that has stopped accepting writes.

`haven-worker` now runs the same job every six hours, and `haven-telemetry-maintain` (same code, one run,
prints the report) exists for the operator who cannot wait for a tick. Creating a month ahead is a
`CREATE TABLE`; dropping an expired month is `DROP TABLE` on a partition, not a mass `DELETE` — which is
the entire reason the table is partitioned rather than merely indexed.

**The failure that made this worth doing carefully.** The first run against the dev database did not
create a partition. It failed:

```text
error returned from database: updated partition constraint for default partition
"telemetry_overflow" would be violated by some row
```

Postgres will not create a partition whose bounds would claim rows the DEFAULT partition already holds —
and there were 9 537 of them, every reading from **October 2026**, because 0003 started partitioning at
the month it ran in (November) and the month before had nowhere to go. That is not a dev-only accident:
any deployment that was reporting before its migrations ran has the same stranded month.

The job therefore lifts those rows aside first — a temporary spill table, then the partition, then the
spill back in through the parent — and reports how many it rescued. After the run:

```text
created=["telemetry_2026_10"] rescued=9537 overflow_rows=0
```

and a query for a day in October now touches one partition instead of the parent plus the overflow.

Two operational details that are not incidental:

- **It connects as the schema owner**, exactly as the migrator does. Creating a partition requires owning
  the parent table, and the application role owns nothing on purpose — that is what makes row-level
  security apply to it. An app-role pool fails every tick with `must be owner of table telemetry`.
- **It is safe on a schedule and on three replicas.** Every step is `IF NOT EXISTS` / `IF EXISTS`, the
  spill is inside one transaction on one connection, and a second run in the same minute does nothing.

Hourly rollups are deleted by the same run, in batches of 40 000 up to 25 batches a tick: `telemetry_rollup`
is not partitioned (one row per point per hour is three orders of magnitude smaller than the readings it
summarises), so its window is enforced with deletes rather than `DROP TABLE`. They go through the
`haven_system` role because that table has RLS forced on and an owner is still subject to policies.

### The seed that lied because the sweep was honest

Seeding a 1 200-device home with `last_seen_at = now() - (g % 40) minutes` produced a dashboard reading
*"0 of 1200 online"*. Nothing was broken: the presence sweep marks anything unheard from for 90 seconds as
offline, and most of the seed was older than that. A fixture that depends on a background job **not**
running is a fixture that lies the moment you start it — the online devices in `seed-huge-home.sql` now sit
five seconds in the past, and the rest are three days old on purpose.

Retention is `TELEMETRY_RETENTION_DAYS`, default **730**. It is a platform term rather than a vendor's:
`licence_grants.retention_days` is about *people*, and letting a vendor's contract decide how long
telemetry lives would mean one vendor's paperwork destroying another vendor's device history.

## Backup and restore

A backup nobody has restored is a hypothesis. `scripts/backup.sh` dumps, verifies the dump with
`pg_restore --list` before renaming it into the set (a `.partial` file is never mistaken for a
backup), and keeps a sliding window; `scripts/restore-drill.sh` restores the newest dump into a
throwaway database, compares row counts per table, checks the newest telemetry point is
byte-identical, and drops the database even when it fails.

Transcript, from a real run on 2026-11-16 against the development database — the numbers are the
point, not the prose:

```
$ make backup
backup: dumping via docker exec haven-db -> .haven/backups/haven-20261116T154743Z.dump
backup: ok — .haven/backups/haven-20261116T154743Z.dump (1.0M, 60 tables of data)

$ make restore-drill
drill: restoring haven-20261116T154743Z.dump into haven_restore_drill
CREATE DATABASE
table                      source   restored
vendors                        17         17  ok
vendor_users                    1          1  ok
products                        9          9  ok
product_serials                22         22  ok
devices                       174        174  ok
device_points                 504        504  ok
commands                      101        101  ok
homes                          73         73  ok
home_members                   58         58  ok
scenes                          0          0  ok
automations                    36         36  ok
audit_log                      12         12  ok
pairing_tickets                80         80  ok
licence_grants                  1          1  ok
licence_activations           169        169  ok
drill: newest device point — source 2026-11-16 14:53:52.958217+00, restored 2026-11-16 14:53:52.958217+00
drill: ok — haven_restore_drill matched the source on 15 tables and 31 migrations
```

What this proves and what it does not:

- **proves** the dump is restorable, the schema it carries is complete (31 migrations), every
  counted table survived with the same number of rows, and no timestamp came back shifted by a
  timezone;
- **does not prove** a point-in-time recovery, an off-site copy, or a restore into a different
  Postgres major version. Those are deployment concerns; this drill answers "can we get the data
  back at all", which is the question a security questionnaire actually asks.

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
