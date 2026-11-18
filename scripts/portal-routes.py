#!/usr/bin/env python3
"""Every path the portal asks for must exist on the server.

Why this exists
---------------
The portal and the API are two trees that only meet at a string. A typo in that string is not a
compile error on either side: TypeScript sees `request('/vendor/suport-bundle')` as a perfectly
good argument, and the server has no idea anybody asked. The page renders an error that says
"not found", which reads like the *device* is missing rather than the *call*, and every test passes
because the portal's tests stub the transport.

So the two ends are compared mechanically, the same way `route-arity.py` compares a route to its
handler:

    every literal `/vendor/...` or `/ops/...` in portal/src  ->  matches a declared route

Matching rules, in the order they matter:
  * `${expr}` in a template literal is a path value, not text: it matches one route segment.
  * A query string is dropped — the server receives it separately, and `?period=${...}` is not part
    of the path.
  * Extra trailing segments in the portal path must themselves be dynamic, so `/vendor/devices${
    query(...)}` is allowed (the helper builds the query string) but `/vendor/devices/extra` is not.

`--self-test` runs the same matcher against a known-bad path and a known-good one, so a checker
that quietly goes blind is itself a red gate.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PORTAL = ROOT / "portal" / "src"
ROUTES = ROOT / "backend" / "crates" / "api" / "src" / "routes"

ROUTE_LITERAL = re.compile(r'\.route\(\s*"([^"]+)"', re.S)

PREFIXES = ("/vendor/", "/ops/")


def _skip_expression(text: str, at: int) -> int:
    """Index just past the `}` closing a `${` that starts at `at` — nested braces included.

    Template literals nest: `/vendor/audit.csv${qs ? `?${qs}` : ''}` holds a backtick inside its own
    `${}`. A scan that stopped at the first backtick would truncate the path and then report a route
    that exists as missing, which is the wrong way round for a guard.
    """
    depth = 0
    i = at
    while i < len(text):
        char = text[i]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return len(text)


def portal_literals(text: str) -> list[str]:
    """Every quoted `/vendor/...` or `/ops/...` in a source file, template expressions included."""
    found: list[str] = []
    i = 0
    while i < len(text):
        char = text[i]
        if char not in "'`\"":
            i += 1
            continue
        quote = char
        start = i + 1
        if not text.startswith(PREFIXES, start):
            i += 1
            continue
        i = start
        out = ""
        while i < len(text):
            char = text[i]
            if quote == "`" and text.startswith("${", i):
                close = _skip_expression(text, i + 1)
                out += text[i:close]
                i = close
                continue
            if char == quote:
                break
            if char == "\\" and i + 1 < len(text):
                out += text[i : i + 2]
                i += 2
                continue
            out += char
            i += 1
        found.append(out)
        i += 1
    return found


def strip_query(path: str) -> str:
    """The path as the router sees it: a query string is not part of the match."""
    for separator in ("?", "#"):
        path = path.split(separator, 1)[0]
    return path


def segments(path: str) -> list[str]:
    """Split on `/`, except inside a `${...}`, and record each expression as one segment."""
    out: list[str] = []
    current = ""
    i = 0
    text = path.strip("/")
    while i < len(text):
        if text.startswith("${", i):
            close = _skip_expression(text, i + 1)
            current += "{}"
            i = close
            continue
        char = text[i]
        if char == "/":
            out.append(current)
            current = ""
        else:
            current += char
        i += 1
    if current:
        out.append(current)
    return [part for part in out if part]


def matches(portal_path: str, route_path: str) -> bool:
    wanted = segments(strip_query(portal_path))
    declared = [part if part.startswith("{") else part for part in segments(route_path)]
    if len(wanted) < len(declared):
        return False
    for got, expected in zip(wanted, declared):
        if expected.startswith("{"):
            continue
        # A value glued to a literal — `/vendor/devices${query(...)}` — is that literal plus a suffix.
        if got.endswith("{}") and got[: -len("{}")] == expected:
            continue
        if got != expected:
            return False
    # Nothing may be appended except a value — `/vendor/devices/extra` is a different route.
    return all(part == "{}" for part in wanted[len(declared) :])


def declared_routes() -> dict[str, str]:
    routes: dict[str, str] = {}
    for source in sorted(ROUTES.glob("*.rs")):
        text = source.read_text()
        for path in ROUTE_LITERAL.findall(text):
            if path.startswith(("/vendor", "/ops")):
                routes.setdefault(path, source.name)
    return routes


def portal_paths() -> dict[str, str]:
    found: dict[str, str] = {}
    for source in sorted(PORTAL.rglob("*")):
        if source.suffix not in (".ts", ".tsx"):
            continue
        for literal in portal_literals(source.read_text()):
            found.setdefault(literal, source.relative_to(ROOT).as_posix())
    return found


def self_test() -> int:
    routes = {"/vendor/team/members/{id}": "vendor.rs", "/vendor/devices": "vendor.rs"}
    cases = [
        ("/vendor/devices", True),
        ("/vendor/devices${query(filter, pageParam)}", True),
        ("/vendor/team/members/${id}", True),
        ("/vendor/team/member/${id}", False),
        ("/vendor/devices/extra", False),
    ]
    bad = []
    for path, expected in cases:
        got = any(matches(path, route) for route in routes)
        if got != expected:
            bad.append(f"{path}: expected {expected}, got {got}")
    if bad:
        print("self-test failed — the matcher no longer tells these apart:", *bad, sep="\n  ")
        return 1
    print("self-test ok")
    return 0


def main() -> int:
    if "--self-test" in sys.argv:
        return self_test()

    routes = declared_routes()
    if len(routes) < 20:
        print(f"only {len(routes)} routes found — the scan is looking in the wrong place")
        return 1

    paths = portal_paths()
    if not paths:
        print("no portal paths found — the scan is looking in the wrong place")
        return 1

    missing = {
        path: source
        for path, source in sorted(paths.items())
        if not any(matches(path, route) for route in routes)
    }

    if missing:
        print("the portal asks for paths the server does not serve:")
        for path, source in missing.items():
            print(f"  {path}   ({source})")
        return 1

    print(f"{len(paths)} portal paths match {len(routes)} declared routes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
