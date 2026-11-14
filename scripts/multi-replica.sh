#!/usr/bin/env bash
# Two workers, one fleet — the case a single-worker drill cannot see.
#
# With replicas, the shared subscription decides who gets each message, and two sweeps can
# race over the same refused command. Both are properties of the *deployment*, not of any one
# process, so they cannot be tested by running one worker very hard.
#
# Usage: scripts/multi-replica.sh
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
API=${API:-http://127.0.0.1:8080}
V1=$API/v1
DB=${DB:-haven}
STATE=$(mktemp -d)
PASS=0
FAIL=0

say()  { printf '\n\033[1m%s\033[0m\n' "$*"; }
note() { printf '  %s\n' "$*"; }
ok()   { PASS=$((PASS + 1)); printf '  \033[32mok\033[0m   %s\n' "$*"; }
bad()  { FAIL=$((FAIL + 1)); printf '  \033[31mFAIL\033[0m %s\n' "$*"; }
json() { python3 -c "import sys,json;d=json.load(sys.stdin);print($1)" 2>/dev/null; }
q()    { docker exec haven-db psql -U "$DB" -d "$DB" -tAc "$1" 2>/dev/null | tr -d ' '; }

cleanup() {
  pkill -f 'target/debug/haven-worker' 2>/dev/null
  docker start haven-mqtt >/dev/null 2>&1
}
trap cleanup EXIT

say "Starting two workers"
pkill -f 'target/debug/haven-worker' 2>/dev/null
sleep 1
cd "$ROOT/backend" || exit 1
set -a; . ../.env; set +a
./target/debug/haven-worker > "$STATE/worker-a.log" 2>&1 &
./target/debug/haven-worker > "$STATE/worker-b.log" 2>&1 &
sleep 8

for name in a b; do
  grep -q "ingest subscription in place" "$STATE/worker-$name.log" \
    && ok "worker $name connected and subscribed" \
    || bad "worker $name did not subscribe: $(tail -2 "$STATE/worker-$name.log" | tr '\n' ' ')"
done

# ---------------------------------------------------------------- shared subscription
say "Shared subscription: each message ingested once"
IDS=$(docker exec haven-db psql -U "$DB" -d "$DB" -tAc \
  "select vendor_id||':'||home_id||':'||id from devices where home_id is not null limit 20" | paste -sd, -)
LIST=$(docker exec haven-db psql -U "$DB" -d "$DB" -tAc \
  "select id from devices where home_id is not null limit 20" | paste -sd, - | sed "s/,/','/g")
docker exec haven-db psql -U "$DB" -d "$DB" -q -c \
  "update device_points set history='all' where device_id::text in ('$LIST')" >/dev/null

before=$(q "select count(*) from telemetry")
published=$(cd "$ROOT/backend" && LOAD_DEVICES="$IDS" LOAD_RATE=60 LOAD_SECONDS=8 ./target/debug/haven-load 2>&1 \
  | sed -n 's/.*published=\([0-9]*\).*/\1/p')
note "published ${published:-0} messages across two workers"

# Wait until the row count stops moving: a short window counts a backlog as loss, which is
# how the first version of this script failed a healthy deployment.
last=-1; still=0
while [ "$still" -lt 18 ]; do
  sleep 3
  now=$(q "select count(*) from telemetry")
  if [ "$now" = "$last" ]; then still=$((still + 3)); else still=0; last=$now; fi
done
landed=$(( $(q "select count(*) from telemetry") - before ))
expected=$(( ${published:-0} * 2 ))

docker exec haven-db psql -U "$DB" -d "$DB" -q -c \
  "update device_points set history='change' where device_id::text in ('$LIST')" >/dev/null

if [ "$expected" -gt 0 ] && [ "$landed" -eq "$expected" ]; then
  ok "no message ingested twice (${landed} rows for ${expected} expected)"
elif [ "$landed" -gt "$expected" ]; then
  bad "$landed rows for $expected expected — messages were ingested more than once"
else
  bad "$landed rows for $expected expected — messages were lost"
fi

# ---------------------------------------------------------------- redelivery race
say "Redelivery: one claim, one publish, two workers"
TOK=$(curl -s -X POST "$V1/auth/register" -H 'content-type: application/json' \
  -d "{\"email\":\"mr-$(date +%s)@example.test\",\"password\":\"mr-Password-1\",\"name\":\"MR\"}" | json 'd["access"]')
HOME_ID=$(curl -s -X POST "$V1/homes" -H "authorization: Bearer $TOK" \
  -H 'content-type: application/json' -d '{"name":"MR flat"}' | json 'd["id"]')
TICKET=$(curl -s -X POST "$V1/homes/$HOME_ID/pairing-tickets" -H "authorization: Bearer $TOK" \
  -H 'content-type: application/json' -d '{}' | json 'd["ticket"]')
DEV=$(curl -s -X POST "$V1/provision/claim" -H 'content-type: application/json' \
  -d "{\"pk\":\"hv_acme_bulb1\",\"sn\":\"MR-$(date +%s)\",\"secret\":\"$(python3 -c 'import secrets;print(secrets.token_hex(16))')\",\"ticket\":\"$TICKET\"}" \
  | json 'd["device_id"]')

# Refuse a command by taking the broker away, then give it back and let both sweeps race.
docker stop haven-mqtt >/dev/null
sleep 2
curl -s -o /dev/null -X POST "$V1/devices/$DEV/commands" -H "authorization: Bearer $TOK" \
  -H 'content-type: application/json' -d '{"code":"switch.power","value":true}'
status=$(q "select status from commands where device_id='$DEV' order by requested_at desc limit 1")
[ "$status" = "unreachable" ] && ok "the command was refused while the broker was down" \
  || bad "command status is $status, expected unreachable"

docker start haven-mqtt >/dev/null

# Redelivery runs on a five-second sweep and only once the worker's event loop has
# reconnected, so the publish can trail the broker coming back by a few seconds. Poll for it
# rather than reading once — the earlier version called it missing while it was merely late.
claims=0
for _ in $(seq 1 40); do
  sleep 1
  claims=$(cat "$STATE"/worker-*.log 2>/dev/null | grep -c "redelivered command=")
  [ "$claims" -ge 1 ] && break
done

attempts=$(q "select attempts from commands where device_id='$DEV' order by requested_at desc limit 1")
# One refused attempt plus one claim. Four would mean both workers acted on one command.
[ "$attempts" = "2" ] && ok "claimed and sent once ($attempts attempts)" \
  || bad "attempts=$attempts — two workers both acted on one command"

# The worker logs the command id, so the count across both logs is the property that matters:
# two lines would be one press delivered twice.
[ "$claims" -eq 1 ] && ok "exactly one worker published it ($claims log line)" \
  || bad "$claims workers published the same command"

# Nothing acks in a drill with no device, so the row settles as `timeout` afterwards. That is
# the platform working, not failing — say so rather than asserting a status a real device
# would have moved on from.
note "row afterwards: $(q "select status from commands where device_id='$DEV' order by requested_at desc limit 1") (no device acks in a drill)"

say "Result"
note "$PASS passed, $FAIL failed"
note "worker logs: $STATE"
[ "$FAIL" -eq 0 ] || exit 1
