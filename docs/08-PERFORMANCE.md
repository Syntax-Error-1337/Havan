# Performance

Measured, not asserted. The numbers below come from `scripts/bench.sh` against a
home with **100 devices × 3 points** and **30 days of history** (17k telemetry
rows), on a laptop with the database in Docker.

Run it yourself:

```bash
./scripts/bench.sh /tmp/tok.txt "$HOME_ID" "$DEVICE_ID"
```

## Baseline (dev laptop, local network)

| Path | Time | Bytes |
|---|---|---|
| `GET /devices?home_id` (100 devices) | 14 ms | 99 KB → **11 KB gzipped** |
| `GET /devices/{id}` | 7 ms | 1 KB |
| `GET /devices/{id}/history?range=1d` | 10 ms | 3.2 KB |
| `GET /devices/{id}/history?range=30d` | 49 ms | 3.6 KB |
| `GET /notifications/unread-count` | 3 ms | 12 bytes |

Two things worth reading out of that table:

- **The device list is 99 KB of repetitive JSON** — a hundred cards' worth of
  name/points/value. Gzip takes it to 11 KB, which is why the API enables
  `CompressionLayer`. Without it, every app cold start on a mobile connection
  downloads a hundred kilobytes to draw a screen that shows a hundred switches.
- **30-day history costs five times a 1-day query** for almost the same response
  size, because it scans 8 640 rows to bucket them. 49 ms locally is fine; the
  thing to watch is that it does not become the screen's bottleneck on a device
  with a slower disk.

### Re-measured after two weeks of change

Same script, same 100-device bench home, run on the same laptop — but with a much larger
`telemetry` table behind it (197k rows from load and chaos runs, not 17k) and the database sharing
the machine with two publishers:

| Path | then | now | Bytes |
|---|---|---|---|
| `GET /devices?home_id` | 14 ms | 17.4 ms | 100 KB |
| `GET /devices/{id}` | 7 ms | 11.1 ms | 1 KB |
| `history?range=1d` | 10 ms | 20.5 ms | 2.9 KB |
| `history?range=30d` | 49 ms | 36.1 ms | 3.6 KB |
| `unread-count` | 3 ms | 3.4 ms | 12 bytes |

Read as a whole, not row by row: every path is in the same order of magnitude, the device list — the
one that matters for a cold app start — moved 3 ms, and 30-day history got *faster*, which is the
effect of the index-only scan over more chunks rather than a change anybody made on purpose.

The 1-day row needed a plan before it deserved a story. `scripts/plans.sql` still shows
**Index Only Scan** over `(device_id, point_id, ts)` with **0 heap fetches**, 0.85 ms of database time
against 197k telemetry rows — so the query did not change; the endpoint's 20 ms is the host and the
HTTP path around it, not a missing index. A timing that doubles without a plan to explain it is how
people start "optimising" a thing that was never slow.

## Query plans

Timings say *how long*; plans say *why*, and they are the thing to re-check after
the data grows. `scripts/plans.sql` runs them; the findings at 100 devices and 17k
telemetry rows:

| Path | Plan |
|---|---|
| device list | Bitmap Index Scan on `devices_home_idx`, then a sort of 100 rows |
| points for the home | Hash join, devices from the index, `device_points` scanned (489 rows in the whole table) |
| history, 1 day | **Index Only Scan** on `(device_id, point_id, ts)` — 1.0 ms for 593 rows |
| history, 30 days | Seq scan of the chunks — correct, because the window covers nearly every row |

Two things worth knowing:

- **The 30-day seq scan is not a missing index.** Forcing `enable_seqscan=off`
  makes the planner use the BRIN on `ts` instead, which is slower — it is reading
  the table because the answer needs the table.
- **The points query scans `device_points`**, which is every point of every tenant.
  That is a planner choice on 489 rows and it will switch to a nested loop on the
  device index as the table grows; it is the one plan here that will change shape
  without anyone touching the code, so it is the one to re-run first.

## What is deliberately not optimised

- **The websocket patches the cache instead of refetching.** `point.changed`
  writes the single value into the query cache; only `command.settled` invalidates
  a query. A home with a hundred reporting devices therefore costs no HTTP
  traffic per event. This was already true — measured here so that a future change
  that "simplifies" it into `invalidateQueries` has a baseline to fail against.
- **Per-point throttling is handled at insert** (`history: interval:N`), not by
  batching in the writer. See `docs/02-THING-MODEL.md`.

## Ingest: `make load`

`make bench` measures reads. `make load` measures the other half — a synthetic field of
devices publishing through the same topics and envelopes a real device uses
(`haven-load`, built on `haven-sdk`, so the wire format cannot drift from the firmware
contract).

The measurement needs every message to produce a row, so the script sets the fixture
points to `history = 'all'` for the duration and restores the exact policies afterwards
(not "change" — a fleet configured for `interval:300` would otherwise be left writing 300×
the rows it was set up for).


### Before batching

| published | achieved | telemetry rows | expected | result |
|---|---|---|---|---|
| 200/s | 153/s | 4 000 | 4 000 | no loss |
| 800/s | 613/s | 5 572 | 16 000 | **65 % shed** |
| 2 000/s | 1 661/s | 7 088 | 59 996 | **88 % shed** |

A single worker topped out around **150–200 messages/second**. Above that the broker did
not block the publisher: VerneMQ dropped the slow subscriber's queue (the compose file pins
`DOCKER_VERNEMQ_MAX_INFLIGHT_MESSAGES: 32`), so a burst was shed, not queued. The pipeline
was honest about it — the row count stopped moving — but the message was gone.

The cause was in `infra::repos::ingest::apply`: **one transaction per report**, one metadata
`SELECT` of the device's points per report, then an `UPDATE` and a conditional `INSERT` per
point, then a fan-out publish. The code carried a `shortcut:` comment predicting exactly
this, and the measurement put the wall where the comment did.

### After batching (`apply_reports`)

Reports are buffered per device (`worker::ingest::Batch` — 250 reports or 250 ms, whichever
comes first) and written in **one transaction**: one metadata query, one `UPDATE
device_points` per point holding the value the batch *ended* on, telemetry as a single
`UNNEST` insert, and one fan-out per point instead of one per report. `interval:N` keeps its
per-row guard, because a throttle is measured against what is already stored and a batch
cannot know that.

| published | achieved | telemetry rows | expected | result |
|---|---|---|---|---|
| 200/s | 153/s | 4 000 | 4 000 | no loss |
| 800/s | 615/s | 15 892 | 16 000 | 0.7 % shed |
| 1 000/s | 769/s | 17 836 | 20 000 | 11 % shed |
| 1 500/s | 1 154/s | 18 494 | 30 000 | 38 % shed |
| 3 000/s | 2 306/s | 17 664 | 59 976 | 70 % shed |

**The same 800/s run went from 65 % shed to 0.7 %.** Sustainable throughput per worker moved
from ~150–200 to roughly **600–700 messages/second** — about 4× — and degradation above that
is gradual rather than a cliff.

What still limits it: one transaction per device per flush, and one Redis fan-out per changed
point per batch. Two levers remain — a larger batch window (latency against throughput) and
more workers, which the shared subscription already supports: 20 000 events/s is now roughly
30 workers rather than 130. The metadata cache the old comment suggested is still unbuilt;
batching removed most of what it would have saved.

Honest limits of the number itself: one connection publishes for every device (kinder to the
broker than a real field), everything is local, and the worker shares the machine with
Postgres, VerneMQ and the generator.

### Re-measured, and a harness bug worth more than the number

Re-running `make load` two weeks later reported **8 % loss at 52/s** — a rate far below any
capacity limit, which is impossible as shedding and therefore a bug in the measurement. It was:
the fixture picked the first fifty devices with a home, including some with **no capability rows**,
whose two points are dropped as unknown. Each of those messages then looked like two lost rows.
Four such devices out of fifty is exactly the 8 % that appeared at 60/s, 300/s and 600/s alike.

`scripts/load.sh` now selects only devices that have both datapoints the generator sends, and says
so if there are none. With that fixed:

| scenario | published | achieved | rows | expected | result |
|---|---|---|---|---|---|
| one generator, 60/s asked | 900 | 50/s | 1 800 | 1 800 | no loss |
| one generator, 600/s asked | 7 553 | 328/s | 15 106 | 15 106 | no loss |
| **two generators, 300/s each** | 12 000 | **522/s** | **24 000** | 24 000 | **no loss** |

The single-process ceiling is the **generator**, not the platform: at 328/s the worker kept up with
every message. With two publishers the aggregate reached 522/s with zero loss on a host under swap
pressure (3.4 of 4 GB) sharing CPU with Postgres, VerneMQ and TimescaleDB from another project.

Two conclusions, stated plainly:

- the shortfall between 522/s and the 600–700/s recorded above is this machine's state today, not a
  regression — the loss column is what says so, and it is zero;
- past the generator's ceiling, though, `make load` cannot answer "how much can the platform take"
  without more publishers. Until then the number to quote is **"zero loss at 522/s with two
  generators"**, and the 600–700/s figure stays as the record of a quieter host.
