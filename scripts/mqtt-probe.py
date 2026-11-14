#!/usr/bin/env python3
"""Minimal MQTT client over TLS, to prove the broker's device credentials.

No third-party packages: MQTT 3.1.1 CONNECT/PUBLISH are a handful of bytes, and the point is to
test the broker rather than a client library.
"""
import socket
import ssl
import struct
import sys


def remaining_length(n):
    out = bytearray()
    while True:
        byte = n % 128
        n //= 128
        if n:
            byte |= 0x80
        out.append(byte)
        if not n:
            return bytes(out)


def utf8(s):
    b = s.encode()
    return struct.pack("!H", len(b)) + b


def connect_packet(client_id, username=None, password=None):
    flags = 0x02  # clean session
    payload = utf8(client_id)
    if username is not None:
        flags |= 0x80
        payload += utf8(username)
    if password is not None:
        flags |= 0x40
        payload += utf8(password)
    body = utf8("MQTT") + bytes([4, flags]) + struct.pack("!H", 30) + payload
    return bytes([0x10]) + remaining_length(len(body)) + body


def subscribe_packet(topic, packet_id=1):
    body = struct.pack("!H", packet_id) + utf8(topic) + bytes([0])  # QoS 0
    return bytes([0x82]) + remaining_length(len(body)) + body


def publish_packet(topic, payload, packet_id=1):
    # QoS 1 (0x32) on purpose: a QoS-0 publish has no acknowledgement, so a refused publish and
    # an accepted one look identical from the client. QoS 1 either comes back PUBACK or the
    # broker closes the connection.
    body = utf8(topic) + struct.pack("!H", packet_id) + payload
    return bytes([0x32]) + remaining_length(len(body)) + body


def read_packet(sock):
    header = sock.recv(1)
    if not header:
        return None, b""
    multiplier, length = 1, 0
    while True:
        b = sock.recv(1)
        if not b:
            return header, b""
        length += (b[0] & 0x7F) * multiplier
        if not b[0] & 0x80:
            break
        multiplier *= 128
    body = b""
    while len(body) < length:
        chunk = sock.recv(length - len(body))
        if not chunk:
            break
        body += chunk
    return header, body


def attempt(host, port, ca, client_id, username, password, topic=None, subscribe=None, label=""):
    ctx = ssl.create_default_context(cafile=ca)
    ctx.check_hostname = True
    try:
        raw = socket.create_connection((host, port), timeout=10)
        sock = ctx.wrap_socket(raw, server_hostname=host)
    except Exception as e:  # noqa: BLE001 - the failure is the result
        print(f"  {label}: TLS failed: {e}")
        return False
    try:
        sock.sendall(connect_packet(client_id, username, password))
        header, body = read_packet(sock)
        if header is None:
            print(f"  {label}: connection closed before CONNACK")
            return False
        code = body[1] if len(body) >= 2 else None
        if header[0] != 0x20:
            print(f"  {label}: unexpected packet {header[0]:#x}")
            return False
        if code != 0:
            print(f"  {label}: CONNACK refused, return code {code}")
            return False
        if subscribe:
            sock.sendall(subscribe_packet(subscribe))
            sock.settimeout(4)
            try:
                header, body = read_packet(sock)
            except (socket.timeout, ConnectionResetError):
                header, body = None, b""
            if header is None:
                print(f"  {label}: no SUBACK (refused)")
            elif header[0] & 0xF0 == 0x90:
                code = body[-1] if body else None
                print(f"  {label}: SUBACK code {code} ({'refused' if code == 0x80 else 'allowed'})")
            else:
                print(f"  {label}: unexpected packet {header[0]:#x}")
            return True

        if topic:
            sock.sendall(publish_packet(topic, b"hello"))
            sock.settimeout(4)
            try:
                header, _ = read_packet(sock)
            except (socket.timeout, ConnectionResetError):
                header = None
            if header is None:
                print(f"  {label}: publish not acknowledged (topic refused)")
                return True
            if header[0] & 0xF0 == 0x40:
                print(f"  {label}: PUBACK — publish allowed")
                return True
        print(f"  {label}: CONNACK accepted")
        return True
    finally:
        sock.close()


if __name__ == "__main__":
    host, port, ca = sys.argv[1], int(sys.argv[2]), sys.argv[3]
    dev, secret, vendor, home, other = sys.argv[4:9]
    print(f"broker {host}:{port}")
    attempt(host, port, ca, "probe-good", dev, secret, label="correct secret")
    attempt(host, port, ca, "probe-bad", dev, "wrong-secret", label="wrong secret")
    attempt(host, port, ca, "probe-anon", None, None, label="no credentials")
    attempt(host, port, ca, "probe-own", dev, secret,
            topic=f"haven/{vendor}/{home}/{dev}/evt", label="publish to own topic")
    attempt(host, port, ca, "probe-sub-wild", dev, secret,
            subscribe="haven/#", label="subscribe to haven/#")
    attempt(host, port, ca, "probe-other", dev, secret,
            topic=f"haven/{vendor}/{home}/{other}/evt", label="publish to a sibling's topic")
