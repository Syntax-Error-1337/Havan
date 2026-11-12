#!/usr/bin/env bash
# How much ingest this stack actually takes, and whether it loses anything.
#
# `make bench` measures reads on a prepared home. This measures the other half: many
# devices reporting at once, through the same topics and envelopes a real device uses,
# while counting what landed in `telemetry` afterwards.
#
# Honest limits, stated up front:
#   - one broker connection publishes for every device, so this is kinder to the broker
#     than a real field (one socket per device);
#   - it runs on a laptop, against a local broker and a local Postgres;
#   - the worker must be up, or nothing is ingested and the loss count will say so.
#
# Usage: scripts/load.sh [rate] [seconds]
set -uo pipefail

RATE=${1:-500}
SECONDS_RUN=${2:-20}
API=${API:-http://127.0.0.1:8080}
DB=${DB:-haven}
DEVICES=${DEVICES:-50}

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }
note() { printf '  %s\n' "$*"; }

# Existing devices from the dev fleet: a load test does not need a fresh home, only real
# rows whose tenant ids a topic can be built from.
LIST=$(docker exec haven-db psql -U haven -d "$DB" -tAc \
  "select vendor_id||':'||home_id||':'||id from devices where home_id is not null limit $DEVICES" \
  | paste -sd, -)

[ -n "$LIST" ] || { echo "no devices to load; run make seed / scripts/bench.sh first"; exit 1; }
COUNT=$(( $(printf '%s' "$LIST" | tr ',' '\n' | wc -l | tr -d ' ') ))

# The measurement needs every message to produce a row, and the default history policy is
# `change`: a generator re-sending the same values would write nothing and the script would
# report a loss that is really the policy doing its job. Set the fixture to `all` for the
# duration, and put it back afterwards.
ids=$(printf '%s' "$LIST" | tr ',' '\n' | cut -d: -f3 | paste -sd, -)
SNAPSHOT=$(mktemp)
docker exec haven-db psql -U haven -d "$DB" -tAc \
  "select device_id||'|'||history from device_points where device_id::text = any(string_to_array('$ids', ','))" \
  > "$SNAPSHOT"
saved=$(cut -d'|' -f2 "$SNAPSHOT" | sort -u | tr '\n' ' ')
docker exec haven-db psql -U haven -d "$DB" -q -c \
  "update device_points set history = 'all' where device_id::text = any(string_to_array('$ids', ','))" >/dev/null
note "history policies for the fixture: ${saved:-none} → all (restored at the end)"

before=$(docker exec haven-db psql -U haven -d "$DB" -tAc "select count(*) from telemetry" | tr -d ' ')

say "Publishing ${RATE}/s for ${SECONDS_RUN}s across ${COUNT} devices"
OUT=$(cd "$(dirname "$0")/../backend" && \
  LOAD_DEVICES="$LIST" LOAD_RATE="$RATE" LOAD_SECONDS="$SECONDS_RUN" \
  ./target/debug/haven-load 2>&1)
echo "$OUT" | grep -E '^published' || { echo "$OUT" | tail -5; exit 1; }

published=$(printf '%s' "$OUT" | sed -n 's/.*published=\([0-9]*\).*/\1/p')
achieved=$(printf '%s' "$OUT" | sed -n 's/.*achieved=\([0-9]*\).*/\1/p')

# Ingest is asynchronous: the worker writes after the broker delivers. Wait until the row
# count stops moving for 15 s, not until a fixed deadline — a fixed window reports a loss
# that is really a backlog, which is how a healthy pipeline gets called broken.
say "Waiting for the worker to drain"
started_drain=$(date +%s)
last=-1
still=0
while [ "$still" -lt 15 ]; do
  sleep 3
  now=$(docker exec haven-db psql -U haven -d "$DB" -tAc "select count(*) from telemetry" | tr -d ' ')
  if [ "$now" = "$last" ]; then
    still=$((still + 3))
  else
    still=0
    last=$now
  fi
  [ $(( $(date +%s) - started_drain )) -gt 600 ] && break
done
note "drained in $(( $(date +%s) - started_drain ))s"

after=$(docker exec haven-db psql -U haven -d "$DB" -tAc "select count(*) from telemetry" | tr -d ' ')
landed=$(( after - before ))
# Each report carries two points, so one published message should produce two rows.
expected=$(( published * 2 ))

# Restore before reporting, so a failed assertion does not leave the dev fleet with a
# history policy it was not configured with.
# Restore exactly what was there, not "change": a fleet that was on `interval:300` and is
# left on `change` writes 300x the rows it was configured to.
if [ -n "${SNAPSHOT:-}" ] && [ -s "$SNAPSHOT" ]; then
  cut -d'|' -f2 "$SNAPSHOT" | sort -u | while read -r policy; do
    [ -n "$policy" ] || continue
    devs=$(grep -c "|$policy$" "$SNAPSHOT" | tr -d ' ')
    [ "$devs" -gt 0 ] || continue
    docker exec haven-db psql -U haven -d "$DB" -q -c \
      "update device_points set history = '$policy'
        where device_id::text = any(string_to_array('$(grep "|$policy$" "$SNAPSHOT" | cut -d'|' -f1 | paste -sd, -)', ','))" >/dev/null
  done
  rm -f "$SNAPSHOT"
fi

say "Result"
note "published      ${published} messages in ${SECONDS_RUN}s"
note "achieved       ${achieved}/s (asked for ${RATE}/s)"
note "telemetry rows ${landed} (expected ${expected}: two points per report)"
note "loss           $(( expected > landed ? expected - landed : 0 )) rows"

if [ "$landed" -lt "$expected" ]; then
  note "SHED: $((expected - landed)) of ${expected} rows never arrived at ${achieved}/s"
  note "     a single QoS-1 subscriber that cannot keep up loses its queue, it does not block"
  note "     the publisher — see docs/08-PERFORMANCE.md"
  exit 1
fi
note "no loss at ${achieved}/s on this machine"
