#!/usr/bin/env python3
"""Watch a device's shadow while a command is sent, and print every payload.

Symptom this was written for: a command settles `applied` with `reported = {"dp":"1","value":true}`
while `device_points` stays `false`. Sampling the retained shadow cannot tell "the device never
published true" from "it published true and then something republished false", so this subscribes
and prints every message with a timestamp.

Usage: scripts/shadow-trace.py <device-id> [--send switch.power=true]
"""
import json
import socket
import struct
import subprocess
import sys
import threading
import time

API = "http://127.0.0.1:8080"
V = "11111111-1111-7111-8111-111111111111"
H = "01a1d936-4e1a-7335-964b-11b6c6c170fe"


def utf8(s):
    b = s.encode()
    return struct.pack("!H", len(b)) + b


def rl(n):
    out = bytearray()
    while True:
        x = n % 128
        n //= 128
        if n:
            x |= 0x80
        out.append(x)
        if not n:
            return bytes(out)


def read_packet(sock):
    head = sock.recv(1)
    if not head:
        return None, b""
    mult, length = 1, 0
    while True:
        b = sock.recv(1)
        if not b:
            return None, b""
        length += (b[0] & 0x7F) * mult
        if not b[0] & 0x80:
            break
        mult *= 128
    body = b""
    while len(body) < length:
        chunk = sock.recv(length - len(body))
        if not chunk:
            break
        body += chunk
    return head, body


def login():
    out = subprocess.run(
        ["curl", "-s", "-X", "POST", f"{API}/v1/auth/login", "-H", "content-type: application/json",
         "-d", '{"email":"demo@haven.local","password":"haven12345"}'],
        capture_output=True, text=True, check=True).stdout
    return json.loads(out)["access"]


def send(device, code, value):
    token = login()
    code_name, raw = code.split("=")
    body = json.dumps({"code": code_name, "value": json.loads(raw)})
    out = subprocess.run(
        ["curl", "-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST",
         f"{API}/v1/devices/{device}/commands", "-H", f"authorization: Bearer {token}",
         "-H", "content-type: application/json", "-d", body],
        capture_output=True, text=True, check=True).stdout
    print(f"[{time.strftime('%H:%M:%S')}] command POST -> {out}", flush=True)


def main():
    device = sys.argv[1]
    action = sys.argv[2] if len(sys.argv) > 2 else None

    s = socket.create_connection(("127.0.0.1", 1883), timeout=10)
    body = (utf8("MQTT") + bytes([4, 0xC2]) + struct.pack("!H", 30) + utf8("shadow-trace")
            + utf8("haven-service") + utf8("dev-mqtt-service-token"))
    s.sendall(bytes([0x10]) + rl(len(body)) + body)
    read_packet(s)

    topic = f"haven/v1/{V}/{H}/{device}/shadow"
    sub = struct.pack("!H", 1) + utf8(topic) + bytes([1])
    s.sendall(bytes([0x82]) + rl(len(sub)) + sub)
    read_packet(s)
    print(f"[{time.strftime('%H:%M:%S')}] subscribed to {topic}", flush=True)

    if action:
        def later():
            time.sleep(4)
            send(device, action, None)
        threading.Thread(target=later, daemon=True).start()

    end = time.time() + 40
    s.settimeout(2)
    while time.time() < end:
        try:
            head, payload = read_packet(s)
        except socket.timeout:
            continue
        if head is None:
            print("connection closed", flush=True)
            break
        if head[0] & 0xF0 != 0x30:
            continue
        # topic length, then topic, then the message
        tlen = struct.unpack("!H", payload[:2])[0]
        msg = payload[2 + tlen:].decode("utf-8", "replace")
        print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)
    s.close()


if __name__ == "__main__":
    main()
