#!/usr/bin/env bash
# The customer journey, over HTTP, against a running stack.
#
# This is what `make e2e` promises and what no unit test can: a real server, a real
# database, a real broker connection, and the sequence a person performs — register,
# create a home, pair a device, send a command. Every step asserts a status code and,
# where it matters, a payload fact.
#
#   ./scripts/e2e.sh                 # against http://127.0.0.1:8080
#   API=http://host:8080 ./scripts/e2e.sh
#
# Needs `make up`, `make migrate`, `make seed`, `make api` and `make worker`.
set -euo pipefail

API="${API:-http://127.0.0.1:8080}"
V1="$API/v1"
TS="$(date +%s)"
EMAIL="e2e-$TS@haven.dev"
SECRET="e2e-Password-$TS"
SN="E2E-$TS"
PRODUCT="${PRODUCT:-hv_acme_bulb1}"

pass=0
fail=0

say() { printf '  %-46s %s\n' "$1" "$2"; }
ok() { pass=$((pass + 1)); say "$1" "ok"; }
bad() { fail=$((fail + 1)); say "$1" "FAIL ($2)"; }

# request <method> <path> [body] [token] — sets CODE and BODY
request() {
  local method="$1" path="$2" body="${3:-}" token="${4:-}" out
  out="$(mktemp)"
  local args=(-s -o "$out" -w '%{http_code}' -X "$method" "$path")
  [ -n "$body" ] && args+=(-H 'content-type: application/json' -d "$body")
  [ -n "$token" ] && args+=(-H "authorization: Bearer $token")
  CODE="$(curl "${args[@]}")"
  # One line: a body with newlines would be split by the caller's `read`.
  BODY="$(tr -d '\n' < "$out")"
  rm -f "$out"
}

# field <key> — reads JSON on stdin; the first element, so a bare object and a list
# both work. The key goes in as argv rather than interpolated into the source.
field() {
  python3 -c 'import sys,json;d=json.load(sys.stdin);k=sys.argv[1];print(d[k] if isinstance(d,dict) else d[0][k])' "$1"
}

expect() { # expect <name> <wanted-status>
  if [ "$CODE" = "$2" ]; then ok "$1"; else bad "$1" "wanted $2, got $CODE ${BODY:0:150}"; fi
}

expect_status_one_of() { # expect_status_one_of <name> <a> <b> ...
  local name="$1"; shift
  local s
  for s in "$@"; do [ "$CODE" = "$s" ] && { ok "$name"; return; }; done
  bad "$name" "wanted one of $*, got $CODE ${BODY:0:150}"
}

echo "e2e against $API"

# 1. the server is up and can reach its own dependencies
request GET "$API/healthz"
expect "health is ok" 200
case "$BODY" in
  *'"db":true'*'"redis":true'*'"mqtt":true'*) ok "db, redis and broker connected" ;;
  *) bad "db, redis and broker connected" "$BODY" ;;
esac

# 2. a customer creates an account
request POST "$V1/auth/register" "{\"email\":\"$EMAIL\",\"password\":\"$SECRET\"}"
expect "register" 201
TOKEN="$(printf '%s' "$BODY" | field access)"

# 3. …and a home
request POST "$V1/homes" '{"name":"E2E flat","timezone":"UTC"}' "$TOKEN"
expect "create home" 201
HOME_ID="$(printf '%s' "$BODY" | field id)"

request GET "$V1/homes" '' "$TOKEN"
expect "list homes" 200
case "$BODY" in
  *'"device_count":0'*) ok "home starts with no devices" ;;
  *) bad "home starts with no devices" "$BODY" ;;
esac

# 4. rooms, and the screen the app paints from one call
request POST "$V1/homes/$HOME_ID/rooms" '{"name":"Kitchen"}' "$TOKEN"
expect "create a room" 201
ROOM_ID="$(printf '%s' "$BODY" | field id)"

request PATCH "$V1/rooms/$ROOM_ID?home_id=$HOME_ID" '{"name":"Pantry"}' "$TOKEN"
expect "rename the room" 204

request GET "$V1/homes/$HOME_ID/dashboard" '' "$TOKEN"
expect "read the dashboard" 200
case "$BODY" in
  *'"Pantry"'*) ok "the dashboard carries the rooms" ;;
  *) bad "the dashboard carries the rooms" "${BODY:0:150}" ;;
esac
case "$BODY" in
  *'"unread_count":'*) ok "the dashboard carries the unread count" ;;
  *) bad "the dashboard carries the unread count" "${BODY:0:150}" ;;
esac

# 5. a pairing ticket, and a device claiming it
request POST "$V1/homes/$HOME_ID/pairing-tickets" '' "$TOKEN"
expect "mint pairing ticket" 201
TICKET="$(printf '%s' "$BODY" | field ticket)"

request POST "$V1/provision/claim" \
  "{\"pk\":\"$PRODUCT\",\"sn\":\"$SN\",\"secret\":\"$SECRET\",\"ticket\":\"not-a-ticket\"}"
expect_status_one_of "a forged ticket is refused" 400 401 403 404

request POST "$V1/provision/claim" \
  "{\"pk\":\"$PRODUCT\",\"sn\":\"$SN\",\"secret\":\"$SECRET\",\"ticket\":\"$TICKET\"}"
expect "claim the device" 201
DEVICE_ID="$(printf '%s' "$BODY" | field device_id)"

# The same device retrying the same claim must not produce a second device.
request POST "$V1/provision/claim" \
  "{\"pk\":\"$PRODUCT\",\"sn\":\"$SN\",\"secret\":\"$SECRET\",\"ticket\":\"$TICKET\"}"
REPLAY_ID="$(printf '%s' "$BODY" | field device_id 2>/dev/null || echo '')"
if [ "$REPLAY_ID" = "$DEVICE_ID" ]; then ok "claiming twice replays the same device"; else bad "claiming twice replays the same device" "$CODE ${BODY:0:150}"; fi

# 6. the device is visible, with the points its product declares
request GET "$V1/devices/$DEVICE_ID" '' "$TOKEN"
expect "read the device" 200
case "$BODY" in
  *'switch.power'*) ok "its points came from the product" ;;
  *) bad "its points came from the product" "$BODY" ;;
esac

# 7. a command is accepted, and can be read back before the device settles it
request POST "$V1/devices/$DEVICE_ID/commands" '{"code":"switch.power","value":true}' "$TOKEN"
expect "send a command" 202
CMD_ID="$(printf '%s' "$BODY" | field command_id)"

request GET "$V1/devices/$DEVICE_ID/commands/$CMD_ID" '' "$TOKEN"
expect "read the command back" 200

# 8. the surfaces the phone draws on its home screen
request GET "$V1/devices?home_id=$HOME_ID" '' "$TOKEN"
expect "list devices in the home" 200

request GET "$V1/devices/$DEVICE_ID/history?code=switch.power&range=1d" '' "$TOKEN"
expect "read history" 200

request GET "$V1/notifications/unread-count" '' "$TOKEN"
expect "unread count" 200

request GET "$V1/devices/$DEVICE_ID/updates" '' "$TOKEN"
expect "firmware status" 200

# 9. and the door is shut on another account
request POST "$V1/auth/register" "{\"email\":\"e2e-$TS-b@haven.dev\",\"password\":\"$SECRET\"}"
OTHER="$(printf '%s' "$BODY" | field access)"

request GET "$V1/devices/$DEVICE_ID" '' "$OTHER"
expect "another account cannot read the device" 404

request POST "$V1/devices/$DEVICE_ID/commands" '{"code":"switch.power","value":true}' "$OTHER"
expect "another account cannot command the device" 404

request GET "$V1/homes/$HOME_ID/dashboard" '' "$OTHER"
expect "another account cannot read the dashboard" 404

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ] || exit 1
