#!/usr/bin/env python3
"""Read the board's serial log without resetting it.

Opening `/dev/cu.usbserial-…` asserts DTR and RTS, and on an ESP8266 board those drive GPIO0 and
EN — which is the auto-reset circuit. So every "diagnostic" that opened the port rebooted the
device, and a device that reboots looks like one that dropped its connection.

Setting `dtr = False` and `rts = False` *before* `open()` avoids the pulse. This prints whatever
the running firmware says, including whether Wi-Fi or MQTT is what fails.

Usage: scripts/serial-watch.py [seconds] [port]
"""
import sys
import time

try:
    import serial
except ImportError:
    sys.exit("pyserial is missing: python3 -m pip install --user pyserial")

seconds = int(sys.argv[1]) if len(sys.argv) > 1 else 120
port = sys.argv[2] if len(sys.argv) > 2 else "/dev/cu.usbserial-A5069RR4"

s = serial.Serial()
s.port = port
s.baudrate = 115200
s.timeout = 1
# Before open: these become the initial line states, so the board is not reset by being watched.
s.dtr = False
s.rts = False
try:
    s.open()
except serial.SerialException as e:
    sys.exit(f"cannot open {port}: {e}")

print(f"listening on {port} for {seconds}s (no reset)", flush=True)
buf = b""
end = time.time() + seconds
while time.time() < end:
    try:
        buf += s.read(1024)
    except serial.SerialException as e:
        print(f"serial error: {e}", flush=True)
        break
s.close()

text = buf.decode("utf-8", "replace")
reset_seen = "haven device: esp-12e" in text
print(f"--- {len(text)} bytes, reset banner seen: {reset_seen} ---")
for line in text.splitlines():
    if line.strip() and sum(c.isprintable() for c in line) >= len(line) * 0.85:
        print(line)
