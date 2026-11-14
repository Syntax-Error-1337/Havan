#!/usr/bin/env bash
# Why will this board not join? Run this instead of guessing.
#
# `pio run -d sdk/esp8266 -e scan -t upload` builds and flashes `src/scan.cpp` — a probe that
# reports what the radio actually sees (every access point with its encryption mode, channel and
# signal), what the SDK was configured to join (`WiFi.printDiag`), and the disconnect reason code,
# which is the only thing that separates "wrong password" from "handshake timed out" from "the AP
# refused this station".
#
# Then:
#
#   python3 scripts/wifi-diag.py
#
# What the answers mean, in the order they usually bite:
#
#   target visible: NO              the AP is 5 GHz only, hidden, or out of range
#   auth: unknown (WPA3/SAE?)       the ESP8266 cannot do SAE; the AP needs a WPA2-only SSID
#   auth: WPA/WPA2 mixed            TKIP in the mix breaks the 4-way handshake on some APs —
#                                   the usual fix is to force WPA2-PSK (AES) on the IoT SSID
#   status 6 / reason 15            the key reached the AP and the handshake timed out: PMF,
#                                   WPA3 transition mode, or a genuinely wrong password
#   status 7, no events at all      the station never attempted; suspect the AP ignoring this MAC
#   reason 8 / BEACON_TIMEOUT       associated, then deauthenticated: roaming between two mesh
#                                   nodes, or a station that starves its own Wi-Fi task
#
# This is a diagnostic script, not a test: it prints, it does not judge, because the judgement is
# which of the lines above the output matches.
set -uo pipefail
PORT=${PORT:-/dev/cu.usbserial-A5069RR4}
SECONDS_TO_READ=${SECONDS_TO_READ:-75}

if [ ! -e "$PORT" ]; then
  echo "no board on $PORT — set PORT=/dev/cu.usbserial-… and try again"
  exit 1
fi

python3 - "$PORT" "$SECONDS_TO_READ" <<'PY'
import sys, time
try:
    import serial
except ImportError:
    sys.exit("pyserial is missing: python3 -m pip install --user pyserial")

port, seconds = sys.argv[1], int(sys.argv[2])
s = serial.Serial(port, 115200, timeout=1)
s.setDTR(False)   # GPIO0 high: normal boot
s.setRTS(True)    # EN low: hold in reset
time.sleep(0.4)
s.setRTS(False)   # release
buf = b""
end = time.time() + seconds
while time.time() < end:
    buf += s.read(1024)
s.close()

# The ROM banner arrives at 74880 baud and reads as noise here; keep printable lines only.
for line in buf.decode("utf-8", "replace").splitlines():
    if line.strip() and sum(c.isprintable() for c in line) >= len(line) * 0.9:
        print(line)
PY
