#!/usr/bin/env python3
"""Measure the two paths docs/08-PERFORMANCE.md makes claims about.

  * the dashboard cursor walk on the 1 200-device Plant home — does page six cost page one?
  * the vendor fleet page — the "millions of devices" surface: does sorting or searching change the cost?

Both are keyset walks, and the claim is the same for both: depth must not matter. That is what this measures.

Credentials come from the environment, because a password in a script is a password in the repository:

    DEMO_PASSWORD=... VENDOR_PASSWORD=... python3 scripts/measure-latency.py

Method notes, because numbers from a laptop need them:
  * medians over repeated runs, never a single call — the first call after a login is cold;
  * the fleet queries are measured **round-robin** rather than one at a time: a machine that gets busy for ten seconds
    would otherwise land entirely on whichever query was running at that moment, and the report would blame the query.
  * every request is HTTP to the real API on this tree, seeded with the demo fleet (1 909 devices, one home of 1 200).
"""

from __future__ import annotations

import json
import os
import statistics
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

API = os.environ.get("HAVEN_API", "http://127.0.0.1:8080")
PLANT = os.environ.get("PLANT_HOME", "01a1fd00-0000-7000-8000-0000000000aa")
VENDOR = os.environ.get("VENDOR_ID", "11111111-1111-7111-8111-111111111111")
DEMO_EMAIL = os.environ.get("DEMO_EMAIL", "demo@haven.local")
ADMIN_EMAIL = os.environ.get("VENDOR_EMAIL", "admin@acme.test")


def call(method: str, path: str, token: str | None = None, body: dict | None = None):
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(API + path, data=data, method=method)
    if data:
        request.add_header("content-type", "application/json")
    if token:
        request.add_header("authorization", f"Bearer {token}")
    started = time.perf_counter()
    with urllib.request.urlopen(request, timeout=60) as response:
        payload = response.read()
    return (time.perf_counter() - started) * 1000, payload


def login(path: str, email: str, password: str, extra: dict | None = None) -> str:
    body = {"email": email, "password": password}
    if extra:
        body.update(extra)
    _, payload = call("POST", path, body=body)
    return json.loads(payload)["access"]


def median(values):
    return round(statistics.median(values), 1)


def dashboard_walk(session: str, runs: int = 5) -> None:
    print("== dashboard cursor walk, 1 200-device home ==")
    pages: dict[int, list[float]] = {}
    body = {}
    for _ in range(runs):
        cursor = None
        for page in range(1, 9):
            path = f"/v1/homes/{PLANT}/dashboard?device_limit=200"
            if cursor:
                path += "&device_cursor=" + urllib.parse.quote(cursor, safe="")
            ms, payload = call("GET", path, session)
            body = json.loads(payload)
            pages.setdefault(page, []).append(ms)
            cursor = body.get("devices_next_cursor")
            if not cursor:
                break
    for page in sorted(pages):
        print(f"  page {page}: {median(pages[page])} ms  ({len(pages[page])} runs)")

    devices = body.get("devices") or []
    if not devices:
        return
    ms, _ = call("GET", f"/v1/devices/{devices[0]['id']}", session)
    print(f"  single device: {round(ms, 1)} ms")

    points = devices[0].get("points") or []
    code = points[0]["code"] if points else "temperature.value"
    history = []
    for _ in range(runs):
        try:
            ms, _ = call("GET", f"/v1/devices/{devices[0]['id']}/history?range=1d&code={code}", session)
        except urllib.error.HTTPError as err:
            print(f"  history 1 day: skipped ({err})")
            return
        history.append(ms)
    print(f"  history 1 day ({code}): {median(history)} ms")


def fleet(session: str, runs: int = 15) -> None:
    print("== vendor fleet, 1 909 devices, 200 per page ==")
    queries = [
        ("default (newest)", "?limit=200"),
        ("serial asc", "?limit=200&sort=serial&dir=asc"),
        ("last_seen desc", "?limit=200&sort=last_seen&dir=desc"),
        ("search HUGE-01", "?limit=200&q=HUGE-01"),
    ]
    samples: dict[str, list[float]] = {label: [] for label, _ in queries}
    for _ in range(runs):
        for label, query in queries:
            ms, _ = call("GET", "/v1/vendor/devices" + query, session)
            samples[label].append(ms)

    for label, _ in queries:
        values = sorted(samples[label])
        p95 = values[max(int(len(values) * 0.95) - 1, 0)]
        print(f"  {label}: median {median(values)} ms, p95 {round(p95, 1)} ms")

    cursor = None
    walk: dict[int, float] = {}
    for page in range(1, 6):
        path = "/v1/vendor/devices?limit=200&sort=serial&dir=asc"
        if cursor:
            path += "&cursor=" + urllib.parse.quote(cursor, safe="")
        pages_body: dict = {}
        runs_ms = []
        for _ in range(5):
            ms, payload = call("GET", path, session)
            pages_body = json.loads(payload)
            runs_ms.append(ms)
        walk[page] = median(runs_ms)
        cursor = pages_body.get("next_cursor")
        if not cursor:
            break
    print("  serial asc walk: " + ", ".join(f"p{page} {ms} ms" for page, ms in walk.items()))


def main() -> int:
    demo_password = os.environ.get("DEMO_PASSWORD")
    vendor_password = os.environ.get("VENDOR_PASSWORD")
    if not demo_password or not vendor_password:
        print("set DEMO_PASSWORD and VENDOR_PASSWORD (the seeded demo logins) — a password in this file is a password "
              "in the repository")
        return 2

    try:
        session = login("/v1/auth/login", DEMO_EMAIL, demo_password)
        dashboard_walk(session)
        staff = login("/v1/vendor/auth/login", ADMIN_EMAIL, vendor_password, {"vendor_id": VENDOR})
        fleet(staff)
    except urllib.error.URLError as err:
        print(f"the API is not answering at {API}: {err}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
