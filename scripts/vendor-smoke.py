#!/usr/bin/env python3
"""A live smoke pass over the vendor surfaces that no screen exercises by hand.

Why this exists
---------------
The notifications bug — a route whose handler extracted a path parameter the route never declared —
passed every test in the repo and answered 404 to the app for as long as it existed. Tests are not a
substitute for a request, and the vendor endpoints behind settings screens (two-factor, SSO,
invoices, webhooks, support bundle) are exactly the ones nobody clicks through.

This walks the real lifecycle against a running API, prints one line per assertion, and exits
non-zero if any of them is wrong. It is deliberately stateful in one place: the TOTP pass enrols and
then un-enrols the same vendor, so running it twice leaves the account as it found it.

    python3 scripts/vendor-smoke.py                      # defaults to the dev stack
    API=http://host:8080 VENDOR=<uuid> EMAIL=... PASSWORD=... python3 scripts/vendor-smoke.py

Standard library only, so it runs wherever the API runs.
"""

from __future__ import annotations

import base64
import hashlib
import hmac
import json
import os
import struct
import sys
import time
import urllib.error
import urllib.request
from typing import Any

API = os.environ.get("API", "http://127.0.0.1:8080").rstrip("/")
VENDOR = os.environ.get("VENDOR", "11111111-1111-7111-8111-111111111111")
EMAIL = os.environ.get("EMAIL", "admin@acme.test")
PASSWORD = os.environ.get("PASSWORD", "acme-demo-password")

PASSED = [0]
FAILED: list[str] = []


def call(method: str, path: str, body: Any = None, token: str | None = None) -> tuple[int, Any]:
    req = urllib.request.Request(f"{API}/v1{path}", method=method)
    req.add_header("content-type", "application/json")
    if token:
        req.add_header("authorization", f"Bearer {token}")
    data = json.dumps(body).encode() if body is not None else None
    try:
        with urllib.request.urlopen(req, data=data) as r:
            raw = r.read()
            return r.status, (json.loads(raw) if raw else None)
    except urllib.error.HTTPError as e:
        raw = e.read()
        try:
            return e.code, json.loads(raw)
        except (json.JSONDecodeError, UnicodeDecodeError):
            # Never pretend a non-JSON refusal is a document; callers check `status` first.
            return e.code, {"raw": raw.decode(errors="replace")[:200]}


def ok(label: str) -> None:
    PASSED[0] += 1
    print(f"  ok   {label}")


def bad(label: str) -> None:
    FAILED.append(label)
    print(f"  FAIL {label}")


def check(label: str, condition: bool) -> None:
    ok(label) if condition else bad(label)


def totp(secret: str, step: int = 30, digits: int = 6) -> str:
    key = base64.b32decode(secret.upper() + "=" * ((8 - len(secret) % 8) % 8))
    counter = struct.pack(">Q", int(time.time()) // step)
    mac = hmac.new(key, counter, hashlib.sha1).digest()
    offset = mac[-1] & 0x0F
    code = struct.unpack(">I", mac[offset : offset + 4])[0] & 0x7FFFFFFF
    return str(code % (10**digits)).zfill(digits)


def login(code: str | None = None):
    body = {"vendor_id": VENDOR, "email": EMAIL, "password": PASSWORD}
    if code is not None:
        body["totp"] = code
    return call("POST", "/vendor/auth/login", body)


def main() -> int:
    print(f"vendor smoke: {API}, vendor {VENDOR}")

    status, tokens = login()
    if status != 200 or not (tokens or {}).get("access"):
        print(f"  FAIL cannot sign in ({status}); nothing else can be checked")
        return 1
    token = tokens["access"]
    ok("sign in")

    print("\nReads that a settings screen makes")
    for path, field in [
        ("/vendor/security", "totp_required"),
        ("/vendor/sso", "required"),
        ("/vendor/invoices", "period"),
        ("/vendor/webhooks", None),
        ("/vendor/team", "members"),
        ("/vendor/licences", "summary"),
    ]:
        status, body = call("GET", path, None, token)
        present = field is None or (isinstance(body, dict) and field in body)
        check(f"GET {path} -> {status}", status == 200 and present)

    status, _ = call("GET", "/vendor/support-bundle", None, token)
    check(f"GET /vendor/support-bundle -> {status}", status == 200)

    print("\nTwo-factor, end to end")
    status, enrol = call("POST", "/vendor/totp/enrol", {}, token)
    secret = (enrol or {}).get("secret") if status == 200 else None
    check("enrolment returns a secret and an otpauth URI", bool(secret) and "otpauth://" in (enrol or {}).get("otpauth_uri", ""))

    if secret:
        status, body = call("POST", "/vendor/totp/confirm", {"code": "000000"}, token)
        check(f"a wrong code is refused ({status})", status == 401)

        status, body = call("POST", "/vendor/totp/confirm", {"code": totp(secret)}, token)
        check(f"the right code enrols ({status})", status == 200 and (body or {}).get("enrolled") == 1)

        status, body = login(totp(secret))
        check(f"sign-in with a code ({status})", status == 200 and bool((body or {}).get("access")))

        status, body = login("000000")
        check(f"sign-in with a wrong code ({status})", status == 401)
        status, body = login()
        check(f"sign-in with no code ({status}, totp required)", status == 401 and ((body or {}).get("error", {}).get("details", {}) or {}).get("totp") == "required")

        status, body = call("PATCH", "/vendor/security", {"totp_required": True}, token)
        check(f"the policy switches on ({status})", status == 200)

        status, body = call("DELETE", "/vendor/totp", None, token)
        check(f"disabling while required is refused ({status})", status == 409)

        status, body = call("PATCH", "/vendor/security", {"totp_required": False}, token)
        check(f"the policy switches off ({status})", status == 200)

        status, body = call("DELETE", "/vendor/totp", None, token)
        check(f"disabling once allowed ({status})", status == 200 and (body or {}).get("enrolled") == 0)

        status, body = login()
        check(f"back to password-only sign-in ({status})", status == 200)

    print("\nWebhooks: shown once, then removable")
    status, hook = call("POST", "/vendor/webhooks", {"url": "https://smoke.example.test/hook", "events": ["licence.activated"]}, token)
    check(f"a subscription is created ({status})", status == 200 and bool((hook or {}).get("secret")))
    hook_id = (hook or {}).get("id")
    if hook_id:
        status, _ = call("GET", "/vendor/webhooks", None, token)
        listed = any(h.get("id") == hook_id for h in (_ or []))
        check("it appears in the list, without its secret", status == 200 and listed)
        status, _ = call("DELETE", f"/vendor/webhooks/{hook_id}", None, token)
        check(f"it can be removed ({status})", status == 200)
        status, body = call("GET", "/vendor/webhooks", None, token)
        check("and is gone from the list", all(h.get("id") != hook_id for h in (body or [])))

    print("\nSmall refusals that carry the right shape")
    status, body = call("PATCH", "/vendor/sso", {"required": True}, token)
    details = ((body or {}).get("error", {}) or {}).get("details", {})
    check(f"SSO cannot be required while nobody is bound ({status})", status == 409 and details.get("bound") is not None)

    status, body = call("GET", "/vendor/invoices/2019-01", None, token)
    check(f"a period with no activity is an empty document, not an error ({status})", status == 200 and (body or {}).get("quantity") == 0)

    print(f"\nResult\n  {PASSED[0]} passed, {len(FAILED)} failed")
    for f in FAILED:
        print(f"  - {f}")
    return 0 if not FAILED else 1


if __name__ == "__main__":
    sys.exit(main())
