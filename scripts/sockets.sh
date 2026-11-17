#!/usr/bin/env bash
# One home, N phones: what the live stream costs Redis, per pod, and in memory.
#
# The fan-out (`api/src/hub.rs`) made one Redis subscription serve every socket on a channel. Its claims are
# measurable, so they are measured here rather than described:
#
#   1. N sockets on one home hold ONE subscription per pod, not N;
#   2. a frame published once reaches every socket, wherever it landed;
#   3. with two replicas, each holds its own single subscription — the cost of a per-process hub, stated;
#   4. the memory a pod pays per socket, before/during/after.
#
# Usage:
#   scripts/sockets.sh                      # 1 000 sockets, one pod
#   scripts/sockets.sh 1000 8080,8090       # 1 000 sockets spread over two pods
#
# Needs the stack, the demo seed and `cargo build -p simulator --bin haven-sockets`.
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
COUNT="${1:-1000}"
PORTS="${2:-8080}"
HOLD="${3:-10}"
HOME_ID="${HOME_ID:-01a1fd00-0000-7000-8000-0000000000aa}"   # "Plant", 1 200 devices

say()  { printf '\n\033[1m%s\033[0m\n' "$*"; }
note() { printf '  %s\n' "$*"; }
rss()  { ps -o rss= -p "$1" 2>/dev/null | awk '{printf "%.0f MB", $1/1024}'; }
pod_pid() { lsof -nP -iTCP:"$1" -sTCP:LISTEN -t 2>/dev/null | head -1; }

FIRST="${PORTS%%,*}"
TOKEN=$(curl -s -X POST "http://127.0.0.1:${FIRST}/v1/auth/login" -H 'content-type: application/json' \
  -d '{"email":"demo@haven.local","password":"haven12345"}' \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["access"])') || {
    echo "could not sign in — is the stack up and seeded?" >&2; exit 1; }

WEBSOCKETS=""
IFS=',' read -ra PARTS <<< "$PORTS"
for port in "${PARTS[@]}"; do
  WEBSOCKETS+="${WEBSOCKETS:+,}ws://127.0.0.1:${port}"
done

say "Before: ${COUNT} sockets over ${#PARTS[@]} pod(s)"
for port in "${PARTS[@]}"; do
  pid=$(pod_pid "$port")
  [ -n "$pid" ] && note "pod :$port  rss $(rss "$pid")"
done
note "redis subscriptions on home:${HOME_ID}: $(docker exec haven-redis redis-cli PUBSUB NUMSUB "home:${HOME_ID}" | tail -1)"

"$ROOT/backend/target/debug/haven-sockets" --token "$TOKEN" --home "$HOME_ID" \
  --count "$COUNT" --urls "$WEBSOCKETS" --hold "$HOLD" > /tmp/sockets.out 2>&1 &
LOAD_PID=$!

sleep 7
say "During (before the frame)"
for port in "${PARTS[@]}"; do
  pid=$(pod_pid "$port")
  [ -z "$pid" ] && { note "pod :$port not running"; continue; }
  channels=$(curl -s "http://127.0.0.1:${port}/metrics" | awk '/^haven_stream_channels/ {print $2}')
  sockets=$(curl -s "http://127.0.0.1:${port}/metrics" | awk '/^haven_stream_sockets/ {print $2}')
  note "pod :$port  sockets ${sockets:-?}  channels ${channels:-?}  rss $(rss "$pid")"
done
note "redis subscriptions on the home channel: $(docker exec haven-redis redis-cli PUBSUB NUMSUB "home:${HOME_ID}" | tail -1)   (one per pod is the design)"

# One publish. Every socket must hear it, whichever pod it landed on.
docker exec haven-redis redis-cli PUBLISH "home:${HOME_ID}" \
  "{\"t\":\"device.online\",\"home_id\":\"${HOME_ID}\",\"device_id\":\"probe\",\"ts\":1794910000}" > /dev/null

wait "$LOAD_PID" || true
say "Result"
cat /tmp/sockets.out

sleep 3
say "After they closed"
for port in "${PARTS[@]}"; do
  pid=$(pod_pid "$port")
  [ -n "$pid" ] && note "pod :$port  rss $(rss "$pid")   (allocator keeps the arena; a second run does not double it)"
done
