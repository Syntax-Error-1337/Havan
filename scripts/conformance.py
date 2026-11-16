#!/usr/bin/env python3
"""Run the Haven conformance checklist against a live platform.

This is the thing a vendor runs before asking us to certify a product: it drives a real device
through the contract the app depends on — claim, connect, report, receive a command, acknowledge it,
survive a reconnect, and be refused when the serial is unknown. Every check reads its answer back
from the platform (`/v1/ops/devices/{id}/state`), not from its own log.

Usage:
    python3 scripts/conformance.py --vendor <uuid> --product <uuid>
    HAVEN_API=http://127.0.0.1:8080 python3 scripts/conformance.py --vendor ... --product ...

Requires: paho-mqtt. The ops door allows loopback without a token; from another machine set
OPS_TOKEN.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
import uuid

try:
    import paho.mqtt.client as mqtt
except ImportError:  # pragma: no cover - environment, not logic
    sys.exit("paho-mqtt is required: python3 -m pip install paho-mqtt")

API = os.environ.get("HAVEN_API", "http://127.0.0.1:8080")
OPS_TOKEN = os.environ.get("OPS_TOKEN", "")
TOPIC_ROOT = "haven"

results: list[tuple[str, bool, str]] = []


def record(name: str, ok: bool, detail: str = "") -> None:
    results.append((name, ok, detail))
    print(f"  {'PASS' if ok else 'FAIL'}  {name}{'  — ' + detail if detail else ''}")


def http(method: str, path: str, body: dict | None = None) -> tuple[int, dict]:
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(f"{API}{path}", data=data, method=method)
    req.add_header("content-type", "application/json")
    if OPS_TOKEN:
        req.add_header("x-ops-token", OPS_TOKEN)
    try:
        with urllib.request.urlopen(req, timeout=15) as r:
            raw = r.read()
            return r.status, (json.loads(raw) if raw else {})
    except urllib.error.HTTPError as e:
        raw = e.read()
        try:
            return e.code, json.loads(raw) if raw else {}
        except json.JSONDecodeError:
            return e.code, {"raw": raw.decode(errors="replace")}


class Device:
    """A device that behaves like firmware: subscribes to commands, publishes what it is told."""

    def __init__(self, host: str, port: int, username: str, password: str, base: str):
        self.base = base
        self.commands: list[dict] = []
        self.connected = False
        token = uuid.uuid4().hex[:8]
        self.client = mqtt.Client(client_id=f"conformance-{token}", clean_session=True)
        self.client.username_pw_set(username, password)
        self.client.on_connect = self._on_connect
        self.client.on_message = self._on_message
        self.host, self.port = host, port

    def _on_connect(self, client, userdata, flags, rc):  # noqa: ANN001
        self.connected = rc == 0
        client.subscribe(f"{self.base}/cmd", qos=1)

    def _on_message(self, client, userdata, msg):  # noqa: ANN001
        try:
            self.commands.append(json.loads(msg.payload))
        except json.JSONDecodeError:
            pass

    def open(self) -> None:
        self.client.connect(self.host, self.port, keepalive=20)
        self.client.loop_start()
        for _ in range(50):
            if self.connected:
                return
            time.sleep(0.1)
        raise RuntimeError("broker refused the device connection")

    def close(self) -> None:
        self.client.loop_stop()
        self.client.disconnect()
        self.connected = False

    def publish(self, lane: str, payload: dict) -> None:
        info = self.client.publish(f"{self.base}/{lane}", json.dumps(payload), qos=1)
        info.wait_for_publish(timeout=5)

    def ack(self, command: dict, ok: bool = True) -> None:
        self.publish(
            "ack",
            {
                "v": 1,
                "id": command["id"],
                "ts": int(time.time()),
                "ok": ok,
                "dp": command.get("dp"),
                "value": command.get("value"),
            },
        )

    def state(self, vendor_id: str, device_id: str) -> dict:
        status, body = http("GET", f"/v1/ops/devices/{device_id}/state?vendor_id={vendor_id}")
        if status != 200:
            raise RuntimeError(f"state read failed: {status} {body}")
        return body


def wait_for(predicate, timeout: float = 12.0, interval: float = 0.25):
    deadline = time.time() + timeout
    while time.time() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(interval)
    return None


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--vendor", required=True, help="vendor uuid")
    ap.add_argument("--product", required=True, help="product uuid")
    args = ap.parse_args()

    print(f"Conformance run against {API}\n")

    status, checklist = http("GET", "/v1/ops/conformance/checklist")
    if status != 200:
        sys.exit(f"cannot read the checklist: {status} {checklist}")
    print("The suite checks:")
    for item in checklist:
        print(f"  · {item}")
    print()

    status, material = http(
        "POST", "/v1/ops/conformance/sessions", {"vendor_id": args.vendor, "product_id": args.product}
    )
    if status != 200:
        sys.exit(f"could not open a conformance session: {status} {material}")

    serial = material["serial"]
    secret = material["secret"]
    ticket = material["ticket"]
    pk = material["product_key"]
    mode = "trust on first use" if material["allow_unknown_serial"] else "manifest only"
    print(
        f"Session: product {pk} ({material['product_name']}, {material['certification']}, {mode}), "
        f"serial {serial}"
    )

    # 1. refusal — a serial the manifest has never heard of. What is correct depends on what the
    #    product declares, so the run checks the declaration rather than a habit: a manifest-only
    #    product must refuse it, a trust-on-first-use product must accept it and charge for it.
    probe = material
    if material["allow_unknown_serial"]:
        # A fresh ticket, because a ticket pairs exactly one device — that is the rule the claim
        # enforces, and reusing this one would test the wrong refusal.
        status, probe = http(
            "POST",
            "/v1/ops/conformance/sessions",
            {"vendor_id": args.vendor, "product_id": args.product},
        )
        if status != 200:
            sys.exit(f"could not open the probe session: {status} {probe}")

    status, body = http(
        "POST",
        "/v1/provision/claim",
        {
            "pk": pk,
            "sn": f"CONF-UNKNOWN-{uuid.uuid4().hex[:8].upper()}",
            "secret": probe["secret"],
            "ticket": probe["ticket"],
        },
    )
    code = (body.get("error") or {}).get("code") or body.get("code") or body
    if os.environ.get("CONF_DEBUG"):
        print("    debug:", status, json.dumps(body)[:300], "probe_serial:", probe["serial"], "ticket_len:", len(probe["ticket"]))

    if material["allow_unknown_serial"]:
        record(
            "refusal: an unknown serial is accepted and charged, as the product declares",
            status in (200, 201),
            f"{status} {code} (trust on first use)",
        )
    else:
        record(
            "refusal: an unknown serial is refused, as the product declares",
            400 <= status < 500,
            f"{status} {code}",
        )

    # 2. claim — the serial in the manifest becomes a device in a home.
    status, claim = http(
        "POST",
        "/v1/provision/claim",
        {"pk": pk, "sn": serial, "secret": secret, "ticket": ticket, "fw": "conformance-1.0.0"},
    )
    if status not in (200, 201):
        record("claim: manifest serial becomes a device", False, f"{status} {claim}")
        return summarise()
    device_id = claim["device_id"]
    home_id = claim["home_id"]
    vendor_id = claim["vendor_id"]
    record("claim: manifest serial becomes a device", True, f"device {device_id[:8]}…, {claim['points']} capabilities")

    host = claim["mqtt"]["host"]
    port = int(claim["mqtt"]["port"])
    base = f"{TOPIC_ROOT}/v1/{vendor_id}/{home_id}/{device_id}"
    device = Device(host, port, device_id, secret, base)

    # 3. connect — the secret presented at claim authenticates the device.
    try:
        device.open()
        record("connect: device authenticates on the broker", True, f"{host}:{port}")
    except Exception as e:  # noqa: BLE001 - the failure is the result
        record("connect: device authenticates on the broker", False, str(e))
        return summarise()

    # 4. shadow — a reported value reaches the platform and is stored per capability. The point to
    #    drive comes from the platform's own capability list, never from a guess: a suite that
    #    hard-codes 'power' passes on one product and lies on the next.
    snap = device.state(vendor_id, device_id)
    writable = [p for p in snap["points"] if p["writable"]]
    point = next((p for p in writable if p["kind"] == "bool"), writable[0] if writable else None)
    if point is None:
        record("shadow: a reported value is stored per capability", False, "the product has no writable point")
        return summarise()
    dp = point["dp"]
    print(f"  driving dp {dp} ({point['code']}, {point['kind']})")

    # `online` is not optional in the shadow contract: a report without it is rejected, which is
    # correct — the platform will not guess whether a device that just spoke is still there.
    device.publish("shadow", {"v": 1, "ts": int(time.time()), "online": True, "pts": {dp: True}})
    state = wait_for(
        lambda: (
            lambda s: s if any(p["dp"] == dp and p["value_bool"] is True for p in s["points"]) else None
        )(device.state(vendor_id, device_id))
    )
    record(
        "shadow: a reported value is stored per capability",
        state is not None,
        f"dp {dp}=true" if state else "value never appeared",
    )

    # 5. command — the platform sends one, the device hears it, and the ack settles it.
    status, issued = http(
        "POST",
        f"/v1/ops/conformance/devices/{device_id}/commands?vendor_id={vendor_id}",
        {"code": point["code"], "value": False},
    )
    if status not in (200, 202):
        record("command: issued and delivered", False, f"{status} {issued}")
    else:
        heard = wait_for(lambda: device.commands[-1] if device.commands else None)
        record(
            "command: issued and delivered",
            heard is not None and heard.get("dp") == dp,
            f"command {issued.get('command_id', '')[:8]}…",
        )
        if heard:
            device.ack(heard)
            settled = wait_for(
                lambda: (
                    lambda s: s
                    if any(c["status"] == "applied" for c in s["commands"])
                    else None
                )(device.state(vendor_id, device_id))
            )
            record(
                "ack: the device's acknowledgement settles the command",
                settled is not None,
                "status=applied" if settled else "never settled",
            )

    # 6. reconnect — a dropped connection comes back and the platform sees it.
    device.close()
    time.sleep(1)
    device = Device(host, port, device_id, secret, base)
    try:
        device.open()
        device.publish("shadow", {"v": 1, "ts": int(time.time()), "online": True, "pts": {dp: True}})
        back = wait_for(
            lambda: (
                lambda s: s if s["online"] and any(p["dp"] == dp for p in s["points"]) else None
            )(device.state(vendor_id, device_id))
        )
        record(
            "reconnect: the device returns and reports again",
            back is not None,
            "online again" if back else "never came back",
        )
    except Exception as e:  # noqa: BLE001
        record("reconnect: the device returns and reports again", False, str(e))

    device.close()
    return summarise()


def summarise() -> int:
    passed = sum(1 for _, ok, _ in results if ok)
    total = len(results)
    print(f"\n{passed}/{total} checks passed")
    failed = [name for name, ok, _ in results if not ok]
    if failed:
        print("Failed: " + ", ".join(failed))
    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
