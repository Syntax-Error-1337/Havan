#!/usr/bin/env bash
# Baseline numbers for the read paths a phone actually calls.
#
# Deliberately measures the shape a real home has rather than a clean database:
# a hundred devices in one home, and thirty days of history on the device being
# charted. Re-run after a change and compare; a number you cannot reproduce is a
# number you cannot defend.
#
#   ./scripts/bench.sh <token-file> <home-id> <device-id>
set -euo pipefail

API="${API:-http://127.0.0.1:8080/v1}"
TOKEN_FILE="${1:?token file}"
HOME_ID="${2:?home id}"
DEVICE_ID="${3:?device id}"
TOKEN="$(cat "$TOKEN_FILE")"
RUNS="${RUNS:-5}"

bench() {
  local name="$1" url="$2" best=999999 total=0 out bytes=0 i
  for ((i = 0; i < RUNS; i++)); do
    out=$(curl -s -o /tmp/bench.body -w "%{time_total}" "$url" -H "authorization: Bearer $TOKEN")
    bytes=$(wc -c < /tmp/bench.body | tr -d ' ')
    total=$(echo "$total + $out" | bc)
    best=$(echo "if ($out < $best) $out else $best" | bc)
  done
  printf '%-26s min %6.3fs  avg %6.3fs  %8d bytes\n' \
    "$name" "$best" "$(echo "scale=3; $total / $RUNS" | bc)" "$bytes"
}

echo "devices in home: $(curl -s "$API/devices?home_id=$HOME_ID" -H "authorization: Bearer $TOKEN" | python3 -c 'import sys,json;print(len(json.load(sys.stdin)))')"
bench "devices?home_id"      "$API/devices?home_id=$HOME_ID"
bench "devices/{id}"         "$API/devices/$DEVICE_ID"
bench "history 1d"           "$API/devices/$DEVICE_ID/history?code=brightness.level&range=1d"
bench "history 30d"          "$API/devices/$DEVICE_ID/history?code=brightness.level&range=30d"
bench "notifications unread" "$API/notifications/unread-count"
