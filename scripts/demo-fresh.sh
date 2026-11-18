#!/usr/bin/env bash
# Make the demo homes look alive.
#
# Why this exists: every seeded device reports once and then goes quiet, so in a walkthrough the app shows six offline
# tiles, a dimmed field on every card and no live gauge — the states the design work was for are the ones nobody can
# see. The silence rule is `max(15 min, 3 × report_interval_secs)`, so a device with an hourly interval reads as live
# for three hours after its last report.
#
# This is a **demo** tool, deliberately not a background service: it moves `last_seen_at` forward, which is exactly the
# kind of lie a production system must never tell about a device. Run it before a demo, not during a test.
#
#   ./scripts/demo-fresh.sh            # every home
#   ./scripts/demo-fresh.sh <home-id>  # one home
set -euo pipefail

CONTAINER="${HAVEN_DB_CONTAINER:-haven-db}"
USER="${HAVEN_DB_USER:-haven}"
DB="${HAVEN_DB_NAME:-haven}"
HOME_ID="${1:-}"

if ! docker exec "$CONTAINER" pg_isready -U "$USER" -d "$DB" >/dev/null 2>&1; then
  echo "demo-fresh: $CONTAINER is not accepting connections (is Docker running?)" >&2
  exit 1
fi

if [ -n "$HOME_ID" ]; then
  WHERE="WHERE home_id = '$HOME_ID'::uuid"
else
  WHERE=""
fi

# Three columns, because the app reads three. `last_seen_at` is the device's own report, `online` is what the header
# counts, and each point's `updated_at` is what the tile's reading and its silence sentence are actually computed from
# — a device whose points are a day old still says "Silent for 1d" no matter how fresh its last_seen_at is.
AFFECTED=$(docker exec "$CONTAINER" psql -U "$USER" -d "$DB" -tAc "
  UPDATE devices SET last_seen_at = now(), online = true $WHERE;
  UPDATE device_points p SET updated_at = now()
    FROM devices d
   WHERE p.device_id = d.id ${WHERE:+AND d.home_id = '$HOME_ID'::uuid};
  SELECT count(*) FROM devices $WHERE;")

echo "demo-fresh: $AFFECTED device(s) now read as live"
echo "demo-fresh: they stay live for max(15 min, 3 × report_interval_secs) — see docs/09-RESILIENCE.md"
