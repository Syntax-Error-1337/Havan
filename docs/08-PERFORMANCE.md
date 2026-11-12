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

