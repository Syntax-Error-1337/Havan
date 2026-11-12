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

## What is deliberately not optimised

- **The websocket patches the cache instead of refetching.** `point.changed`
  writes the single value into the query cache; only `command.settled` invalidates
  a query. A home with a hundred reporting devices therefore costs no HTTP
  traffic per event. This was already true — measured here so that a future change
  that "simplifies" it into `invalidateQueries` has a baseline to fail against.
- **Per-point throttling is handled at insert** (`history: interval:N`), not by
  batching in the writer. See `docs/02-THING-MODEL.md`.
