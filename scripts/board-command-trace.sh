#!/usr/bin/env bash
# Send a command to the board while its serial line is open, and print what it does.
#
# Opening /dev/cu.usbserial resets the ESP8266 (DTR/RTS drive EN and GPIO0), so a "the device
# ignores commands" report has to be checked with the port open for the whole experiment —
# otherwise the reset that the observation itself causes looks like a bug in the device.
set -uo pipefail

DEV=${DEV:-01a1d941-b91c-715c-ba90-758b66357c84}
PORT=${PORT:-/dev/cu.usbserial-A5069RR4}
API=${API:-http://127.0.0.1:8080}
SECONDS_READ=${SECONDS_READ:-70}
DELAY=${DELAY:-25}

send_command() {
  sleep "$DELAY"
  local token
  token=$(curl -s -X POST "$API/v1/auth/login" -H 'content-type: application/json' \
    -d '{"email":"demo@haven.local","password":"haven12345"}' \
    | python3 -c 'import sys,json;print(json.load(sys.stdin)["access"])')
  curl -s -o /dev/null -X POST "$API/v1/devices/$DEV/commands" -H "authorization: Bearer $token" \
    -H 'content-type: application/json' -d '{"code":"switch.power","value":true}'
  echo "[$(date +%H:%M:%S)] command sent (switch.power = true)"
}

send_command &
SENDER=$!

python3 - "$PORT" "$SECONDS_READ" <<'PY'
import sys, time
import serial

port, seconds = sys.argv[1], int(sys.argv[2])
s = serial.Serial(port, 115200, timeout=1)
s.setDTR(False)   # GPIO0 high: normal boot
s.setRTS(True)    # EN low: reset
time.sleep(0.4)
s.setRTS(False)   # release: run
buf = b""
end = time.time() + seconds
while time.time() < end:
    buf += s.read(1024)
s.close()

for line in buf.decode("utf-8", "replace").splitlines():
    if line.strip() and sum(c.isprintable() for c in line) >= len(line) * 0.85 and "joining" not in line:
        print(line)
PY

wait "$SENDER"
