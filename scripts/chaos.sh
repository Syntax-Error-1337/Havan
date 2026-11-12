#!/usr/bin/env bash
# What breaks when one dependency dies.
#
# The point is not that things fail — they must — but *how*. A dependency that is down has
# to produce a refusal a client can act on (503 with a reason), never a 500 with a stack,
# never a silent success that loses the write, and the service has to come back on its own
# when the dependency does.
#
# Usage: scripts/chaos.sh [db|redis|mqtt|all] [--restore]
#
# The containers are ours and local (deploy/docker-compose.yml): the script stops and
# starts them, which is the crudest possible fault injection and also the only one that
# needs no extra tooling.
set -uo pipefail

API=${API:-http://127.0.0.1:8080}
V1=$API/v1
STATE=$(mktemp -d)
PASS=0
FAIL=0
WHAT=${1:-all}

say()  { printf '\n\033[1m%s\033[0m\n' "$*"; }
note() { printf '  %s\n' "$*"; }
ok()   { PASS=$((PASS + 1)); printf '  \033[32mok\033[0m   %s\n' "$*"; }
bad()  { FAIL=$((FAIL + 1)); printf '  \033[31mFAIL\033[0m %s\n' "$*"; }

json() { python3 -c "import sys,json;d=json.load(sys.stdin);print($1)" 2>/dev/null; }

health_field() {
  curl -s --max-time 3 "$API/healthz" | json "d['components']['$1']"
}

wait_healthy() {
  local want=$1 seconds=${2:-40} i=0
  while [ $i -lt "$seconds" ]; do
    case "$want" in
      all) [ "$(health_field db)" = "True" ] && [ "$(health_field redis)" = "True" ] \
             && [ "$(health_field mqtt)" = "True" ] && return 0 ;;
      *)   [ "$(health_field "$want")" = "True" ] && return 0 ;;
    esac
    sleep 1
    i=$((i + 1))
  done
  return 1
}

container() {
  case "$1" in
    db) echo haven-db ;;
    redis) echo haven-redis ;;
    mqtt) echo haven-mqtt ;;
  esac
}

stop() { docker stop "$(container "$1")" >/dev/null 2>&1; }
start() { docker start "$(container "$1")" >/dev/null 2>&1; }

# A logged-in customer with a home and a device, created while everything is up.
seed() {
  local email="chaos-$(date +%s)@example.test"
  local token home
  token=$(curl -s -X POST "$V1/auth/register" -H 'content-type: application/json' \
    -d "{\"email\":\"$email\",\"password\":\"chaos-Password-1\",\"name\":\"Chaos\"}" | json 'd["access"]')
  [ -n "$token" ] || { bad "could not register a customer"; return 1; }
  echo "$token" > "$STATE/token"

  home=$(curl -s -X POST "$V1/homes" -H "authorization: Bearer $token" \
    -H 'content-type: application/json' -d '{"name":"Chaos flat"}' | json 'd["id"]')
  echo "$home" > "$STATE/home"

  local ticket device sn secret
  sn="CHAOS-$(date +%s)"
  secret=$(python3 -c 'import secrets;print(secrets.token_hex(16))')
  ticket=$(curl -s -X POST "$V1/homes/$home/pairing-tickets" -H "authorization: Bearer $token" \
    -H 'content-type: application/json' -d '{}' | json 'd["ticket"]')
  # Provisioning speaks the device's language (pk/sn/secret), not the app's.
  device=$(curl -s -X POST "$V1/provision/claim" -H 'content-type: application/json' \
    -d "{\"pk\":\"hv_acme_bulb1\",\"sn\":\"$sn\",\"secret\":\"$secret\",\"ticket\":\"$ticket\"}" \
    | json 'd["device_id"]')
  echo "$device" > "$STATE/device"
  [ -n "$device" ] && return 0
  bad "could not claim a device while healthy"
  return 1
}

# One command, and the status it answers with. This is the measurement that matters:
# "accepted" while the broker is down is a lost command nobody will notice.
command_status() {
  local token device
  token=$(cat "$STATE/token" 2>/dev/null)
  device=$(cat "$STATE/device" 2>/dev/null)
  curl -s -o "$STATE/cmd.json" -w '%{http_code}' --max-time 5 \
    -X POST "$V1/devices/$device/commands" -H "authorization: Bearer $token" \
    -H 'content-type: application/json' \
    -d '{"code":"switch.power","value":true}'
}

probe_status() {
  curl -s -o /dev/null -w '%{http_code}' --max-time 5 "$@"
}

trap 'start db; start redis; start mqtt' EXIT

say "Baseline — everything up"
wait_healthy all 60 || bad "the stack was not healthy before the drill started"
[ -f "$STATE/token" ] || seed || exit 1
note "login:      $(probe_status -X POST "$V1/auth/login" -H 'content-type: application/json' \
  -d '{"email":"nobody@example.test","password":"wrong-Password-1"}') (401 expected)"
note "command:    $(command_status) (202 expected)"
ok "baseline recorded"

# ---------------------------------------------------------------- redis
if [ "$WHAT" = all ] || [ "$WHAT" = redis ]; then
  say "Redis down"
  stop redis
  sleep 2
  h=$(health_field redis)
  [ "$h" = "False" ] && ok "/healthz reports redis: false" \
    || bad "/healthz still claims redis: $h"
  s=$(probe_status -X POST "$V1/auth/login" -H 'content-type: application/json' \
    -d '{"email":"nobody@example.test","password":"wrong-Password-1"}')
  # A rate limiter that cannot count must not become a rate limiter that locks everyone
  # out: fail open, let the request through to the password check.
  [ "$s" = "401" ] && ok "login still answers, off the password check ($s)" \
    || bad "login answered $s with redis down (503 would mean the limiter is in the path)"
  s=$(command_status)
  [ "$s" = "202" ] && ok "commands still dispatch ($s)" || bad "commands answered $s"

  start redis
  wait_healthy redis 45 && ok "redis recovered without restarting the API" \
    || bad "redis did not recover"
fi

# ---------------------------------------------------------------- mqtt
if [ "$WHAT" = all ] || [ "$WHAT" = mqtt ]; then
  say "Broker down"
  stop mqtt
  sleep 2
  h=$(health_field mqtt)
  [ "$h" = "False" ] && ok "/healthz reports mqtt: false" \
    || bad "/healthz still claims mqtt: $h"
  s=$(command_status)
  # 503 is the honest answer: we cannot deliver, so do not pretend we did. 202 here would
  # be a command the customer believes was sent, and nothing would ever deliver it.
  if [ "$s" = "503" ]; then
    ok "commands refuse rather than report a false send ($s)"
    refused=$(python3 -c "import re,sys;m=re.search(r'command ([0-9a-f-]{36})', open('$STATE/cmd.json').read());print(m.group(1) if m else '')" 2>/dev/null)
    echo "$refused" > "$STATE/refused"
    grep -q "not sent" "$STATE/cmd.json" 2>/dev/null \
      && ok "the refusal says the command was not sent" \
      || bad "the refusal does not say what happened: $(head -c 120 "$STATE/cmd.json")"
    # And the row must not be left for the timeout sweeper to blame the device.
    settled=$(docker exec haven-db psql -U haven -d haven -tAc \
      "select status || '/' || coalesce(error_code,'-') from commands order by requested_at desc limit 1" 2>/dev/null | tr -d ' ')
    [ "$settled" = "unreachable/broker_unavailable" ] \
      && ok "the command is settled as unreachable ($settled), not left to time out" \
      || bad "the command row reads $settled — the sweeper would later blame the device"
  else
    bad "commands answered $s with the broker down — a lost command looks accepted"
  fi
  s=$(probe_status "$V1/homes")
  [ "$s" = "401" ] && ok "reads are unaffected ($s)" || bad "reads answered $s"

  start mqtt
  wait_healthy mqtt 45 && ok "broker recovered" || bad "broker did not recover"
  s=$(command_status)
  [ "$s" = "202" ] && ok "commands dispatch again ($s)" || bad "commands answered $s after recovery"

  # The refused command is not the customer's problem: the worker delivers it when the
  # broker comes back, within the two-minute window. This is the assertion that makes the
  # 503 honest — "not sent" is only acceptable if it is later sent.
  refused=$(cat "$STATE/refused" 2>/dev/null)
  waited=0
  [ -n "$refused" ] || bad "could not tell which command was refused"
  until [ "$(docker exec haven-db psql -U haven -d haven -tAc \
      "select status from commands where id='$refused'" 2>/dev/null | tr -d ' ')" = "dispatched" ]; do
    [ "$waited" -ge 20 ] && break
    sleep 1
    waited=$((waited + 1))
  done
  [ "$waited" -lt 20 ] && ok "the refused command was delivered once the broker returned (${waited}s)" \
    || bad "the refused command was never redelivered — a 503 that loses the command"
fi

# ---------------------------------------------------------------- db
if [ "$WHAT" = all ] || [ "$WHAT" = db ]; then
  say "Postgres down"
  stop db
  sleep 3
  h=$(health_field db)
  [ "$h" = "False" ] && ok "/healthz reports db: false" || bad "/healthz still claims db: $h"
  s=$(probe_status "$V1/homes" -H "authorization: Bearer $(cat "$STATE/token")")
  # 503 and a JSON body; a 500 with a driver message is an information leak.
  [ "$s" = "503" ] && ok "reads fail as 503 ($s)" || bad "reads answered $s with the database down"
  grep -qi "sqlx\|postgres error\|panicked" "$STATE/cmd.json" 2>/dev/null \
    && bad "the refusal body leaks driver detail" || ok "no driver detail in the body"

  start db
  wait_healthy db 90 && ok "database recovered" || bad "database did not recover"
  s=$(probe_status "$V1/homes" -H "authorization: Bearer $(cat "$STATE/token")")
  [ "$s" = "200" ] && ok "reads work again without an API restart ($s)" \
    || bad "reads answered $s after the database came back"
fi

say "Result"
note "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
