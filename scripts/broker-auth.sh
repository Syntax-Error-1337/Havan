#!/usr/bin/env bash
# Broker authentication, asserted rather than eyeballed.
#
# `scripts/mqtt-probe.py` proves the 8884 listener authenticates a device and confines it to its own
# topics. This runs it against a device the script creates, and fails if any of the six outcomes is
# wrong — because the interesting failures here are silent: a permissive ACL, a hook that is never
# consulted, and a listener that accepts anonymous clients all look like success from the client.
#
# Usage: scripts/broker-auth.sh   (needs the stack up: make up)
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
API=${API:-http://127.0.0.1:8080}
V1=$API/v1
BROKER_HOST=${BROKER_HOST:-127.0.0.1}
BROKER_PORT=${BROKER_PORT:-8884}
CA=${CA:-$ROOT/deploy/certs/ca.pem}
DB=${DB:-haven}
PASS=0
FAIL=0

say()  { printf '\n\033[1m%s\033[0m\n' "$*"; }
ok()   { PASS=$((PASS + 1)); printf '  \033[32mok\033[0m   %s\n' "$*"; }
bad()  { FAIL=$((FAIL + 1)); printf '  \033[31mFAIL\033[0m %s\n' "$*"; }
json() { python3 -c "import sys,json;d=json.load(sys.stdin);print($1)" 2>/dev/null; }
q()    { docker exec haven-db psql -U "$DB" -d "$DB" -tAc "$1" 2>/dev/null | tr -d ' '; }

say "A device, a home, and a sibling"
EMAIL="ba-$(date +%s)@example.test"
TOK=$(curl -s -X POST "$V1/auth/register" -H 'content-type: application/json' \
  -d "{\"email\":\"$EMAIL\",\"password\":\"ba-Password-1\",\"name\":\"BA\"}" | json 'd["access"]')
HOME_ID=$(curl -s -X POST "$V1/homes" -H "authorization: Bearer $TOK" \
  -H 'content-type: application/json' -d '{"name":"BA flat"}' | json 'd["id"]')
if [ -z "${HOME_ID:-}" ]; then
  echo "could not create a home — is the API up on $API?"; exit 1
fi

claim() {
  local ticket secret
  ticket=$(curl -s -X POST "$V1/homes/$HOME_ID/pairing-tickets" -H "authorization: Bearer $TOK" \
    -H 'content-type: application/json' -d '{}' | json 'd["ticket"]')
  secret=$(python3 -c 'import secrets;print(secrets.token_hex(16))')
  curl -s -X POST "$V1/provision/claim" -H 'content-type: application/json' \
    -d "{\"pk\":\"hv_acme_bulb1\",\"sn\":\"BA-$(date +%s%N)\",\"secret\":\"$secret\",\"ticket\":\"$ticket\"}" \
    | json 'd["device_id"]'
  echo "$secret"
}

read -r DEV SECRET <<< "$(claim | paste -sd' ' -)"
read -r OTHER _ <<< "$(claim | paste -sd' ' -)"
VENDOR=$(q "select vendor_id from devices where id='$DEV'")
HOME_OF=$(q "select home_id from devices where id='$DEV'")

if [ -z "${DEV:-}" ] || [ -z "${OTHER:-}" ]; then
  echo "could not claim two devices"; exit 1
fi
printf '  device %s\n  sibling %s\n' "${DEV:0:8}…" "${OTHER:0:8}…"

say "The listener's answers"
OUT=$(python3 "$ROOT/scripts/mqtt-probe.py" "$BROKER_HOST" "$BROKER_PORT" "$CA" \
  "$DEV" "$SECRET" "$VENDOR" "$HOME_OF" "$OTHER" 2>&1)
printf '%s\n' "$OUT" | sed 's/^/  /'

expect() { # expect <substring> <description>
  if printf '%s' "$OUT" | grep -qF "$1"; then ok "$2"; else bad "$2 (looked for: $1)"; fi
}

expect "correct secret: CONNACK accepted"        "its own secret is accepted"
expect "wrong secret: CONNACK refused"           "a wrong secret is refused"
expect "no credentials: CONNACK refused"         "anonymous is refused"
expect "publish to own topic: PUBACK"            "it may publish inside its own prefix"
expect "subscribe to haven/#: SUBACK code 128"   "it may not subscribe to everything"
expect "sibling's topic: publish not acknowledged" "it may not publish to another device"

# The refusal has to come from the platform's decision, not from a broken endpoint: a hook that
# cannot be reached would also produce a refusal, and would be a guess rather than a policy.
refusals=$(grep -c "refused a topic outside the device's own" "${API_LOG:-/dev/null}" 2>/dev/null || true)
if [ -n "${API_LOG:-}" ]; then
  if [ "${refusals:-0}" -ge 1 ]; then ok "the refusal is the platform's ACL, not a dead endpoint"; else bad "no ACL refusal in $API_LOG"; fi
else
  printf '  \033[33mskip\033[0m set API_LOG=<api log> to assert the refusal came from the ACL\n'
fi

say "Result"
printf '  %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
