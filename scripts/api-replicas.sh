#!/usr/bin/env bash
# Two API instances, one database — the health question a single process cannot answer.
#
# `/healthz` is per process, so the failure worth testing is disagreement: one replica saying
# the database is fine while the other says it is gone means a load balancer sends half the
# traffic into a wall. And a replica that never connected to the broker must not claim it has.
#
# Usage: scripts/api-replicas.sh
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
STATE=$(mktemp -d)
PASS=0
FAIL=0
A_PORT=${A_PORT:-8090}
B_PORT=${B_PORT:-8091}

say()  { printf '\n\033[1m%s\033[0m\n' "$*"; }
note() { printf '  %s\n' "$*"; }
ok()   { PASS=$((PASS + 1)); printf '  \033[32mok\033[0m   %s\n' "$*"; }
bad()  { FAIL=$((FAIL + 1)); printf '  \033[31mFAIL\033[0m %s\n' "$*"; }

health() { curl -s --max-time 4 "http://127.0.0.1:$1/healthz"; }
field()  { health "$1" | python3 -c "import sys,json;d=json.load(sys.stdin);print(d['components']['$2'])" 2>/dev/null; }
code()   { curl -s -o /dev/null -w '%{http_code}' --max-time 4 "http://127.0.0.1:$1/healthz"; }
stop()   { pkill -f "HAVEN_REPLICA=$1" 2>/dev/null; }

start_api() {
  local port=$1 tag=$2
  cd "$ROOT/backend" || exit 1
  set -a; . ../.env; set +a
  HAVEN_REPLICA="$tag" HAVEN_BIND="127.0.0.1:$port" HAVEN_CA_DIR="$ROOT/.haven/ca" \
    ./target/debug/haven-api > "$STATE/api-$tag.log" 2>&1 &
  echo $! > "$STATE/api-$tag.pid"
}

stop_all() {
  for tag in a b c; do
    [ -f "$STATE/api-$tag.pid" ] && kill "$(cat "$STATE/api-$tag.pid")" 2>/dev/null
  done
  docker start haven-db >/dev/null 2>&1
  docker start haven-mqtt >/dev/null 2>&1
}
trap stop_all EXIT

wait_field() {
  local port=$1 field_name=$2 want=$3 seconds=${4:-40} i=0
  while [ $i -lt "$seconds" ]; do
    [ "$(field "$port" "$field_name")" = "$want" ] && return 0
    sleep 1
    i=$((i + 1))
  done
  return 1
}

say "Two replicas"
start_api "$A_PORT" a
start_api "$B_PORT" b
sleep 8

for port in "$A_PORT" "$B_PORT"; do
  [ "$(code "$port")" = "200" ] && ok "replica on :$port is healthy (200)" \
    || bad "replica on :$port answered $(code "$port")"
done

# ---------------------------------------------------------------- database goes away
say "Postgres stops"
docker stop haven-db >/dev/null
for port in "$A_PORT" "$B_PORT"; do
  if wait_field "$port" db False 45; then
    ok "replica on :$port reports db: false"
  else
    bad "replica on :$port still claims db: $(field "$port" db)"
  fi
done
# Both must fence themselves out of a load balancer, not just report the field.
for port in "$A_PORT" "$B_PORT"; do
  [ "$(code "$port")" = "503" ] && ok "replica on :$port answers 503 while degraded" \
    || bad "replica on :$port answered $(code "$port") — a load balancer would keep it in"
done

# The two must agree; disagreement is the failure this drill exists for.
a=$(field "$A_PORT" db); b=$(field "$B_PORT" db)
[ "$a" = "$b" ] && ok "both replicas agree about the database ($a)" \
  || bad "replicas disagree: one says db=$a, the other db=$b"

say "Postgres returns"
docker start haven-db >/dev/null
for port in "$A_PORT" "$B_PORT"; do
  if wait_field "$port" db True 60; then
    ok "replica on :$port recovered without a restart"
  else
    bad "replica on :$port did not recover"
  fi
done

# ---------------------------------------------------------------- a replica that never connected
say "A replica started while the broker is down"
docker stop haven-mqtt >/dev/null
sleep 2
start_api 8092 c
sleep 8
[ "$(field 8092 mqtt)" = "False" ] \
  && ok "the new replica does not claim mqtt: true (it never connected)" \
  || bad "the new replica claims mqtt: $(field 8092 mqtt) without ever connecting"

[ "$(field "$A_PORT" mqtt)" = "False" ] && ok "the connected replica noticed the broker is gone" \
  || bad "replica on :$A_PORT still claims mqtt: $(field "$A_PORT" mqtt)"

docker start haven-mqtt >/dev/null
wait_field "$A_PORT" mqtt True 45 && ok "the connected replica recovered on its own" \
  || bad "replica on :$A_PORT did not reattach"

say "Result"
note "$PASS passed, $FAIL failed"
note "logs: $STATE"
[ "$FAIL" -eq 0 ] || exit 1
