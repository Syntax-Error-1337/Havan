#!/usr/bin/env python3
"""Every path a client asks for must exist on the server.

Why this exists
---------------
The clients and the API are separate trees that only meet at a string. A typo in that string is not a
compile error on either side: TypeScript sees `request('/vendor/suport-bundle')` as a perfectly good
argument, and the server has no idea anybody asked. The screen then renders "not found", which reads
like the *device* is missing rather than the *call*, and every test passes because the clients stub
their transport.

So the ends are compared mechanically, the same way `route-arity.py` compares a route to its handler:

    portal/src  literals beginning /vendor or /ops   ->  a declared route under /vendor or /ops
    app/src     the first argument of request(...)   ->  a declared customer route

The app is scanned by call site rather than by literal, because it is full of `/settings`-shaped
strings that are navigation, not API paths. Only what is handed to `request` counts.

Matching rules, in the order they matter:
  * `${expr}` in a template literal is a path value, not text: it matches one route segment.
  * A query string is dropped — the server receives it separately, and `?period=${...}` is not part
    of the path.
  * Extra trailing segments in the client path must themselves be values, so `/vendor/devices${
    query(...)}` is allowed (the helper builds the query string) but `/vendor/devices/extra` is not.

`--self-test` runs the matcher against known-bad paths and the extractor against a nested template,
so a checker that quietly goes blind is itself a red gate. The first version of this file
mis-segmented three real paths and reported routes that exist as missing; the self-test exists
because of that, not for symmetry.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PORTAL = ROOT / "portal" / "src"
APP = ROOT / "app" / "src"
# Routes are declared in `routes/*.rs` and, for a few that predate that split, in `app.rs`. Scanning
# one directory missed `/devices/{id}/history` and reported a client for a route that exists.
API_SRC = ROOT / "backend" / "crates" / "api" / "src"

ROUTE_LITERAL = re.compile(r'\.route\(\s*"([^"]+)"', re.S)
PORTAL_PREFIXES = ("/vendor/", "/ops/")
REQUEST_CALL = re.compile(r"(?<![\w.])request(?:<[^>]*>)?\(\s*")


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


def literal_at(text: str, start: int) -> tuple[str, int] | None:
    """Read the string literal beginning at `start`; its contents and the index just past it."""
    if start >= len(text) or text[start] not in "'`\"":
        return None
    quote = text[start]
    i = start + 1
    out = ""
    while i < len(text):
        char = text[i]
        if quote == "`" and text.startswith("${", i):
            close = _skip_expression(text, i + 1)
            out += text[i:close]
            i = close
            continue
        if char == quote:
            return out, i + 1
        if char == "\\" and i + 1 < len(text):
            out += text[i : i + 2]
            i += 2
            continue
        out += char
        i += 1
    return None


def portal_literals(text: str) -> list[str]:
    """Every quoted `/vendor/...` or `/ops/...` in a source file, template expressions included."""
    found: list[str] = []
    i = 0
    while i < len(text):
        if text[i] in "'`\"" and text.startswith(PORTAL_PREFIXES, i + 1):
            read = literal_at(text, i)
            if read:
                found.append(read[0])
                i = read[1]
                continue
        i += 1
    return found


def app_literals(text: str) -> list[str]:
    """The path argument of every `request(...)` — and nothing else in the file."""
    found: list[str] = []
    for match in REQUEST_CALL.finditer(text):
        read = literal_at(text, match.end())
        if read and read[0].startswith("/"):
            found.append(read[0])
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


def matches(client_path: str, route_path: str) -> bool:
    wanted = segments(strip_query(client_path))
    declared = segments(route_path)
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
    for source in sorted(API_SRC.rglob("*.rs")):
        for path in ROUTE_LITERAL.findall(source.read_text()):
            routes.setdefault(path, source.name)
    return routes


def literals(tree: Path, extract) -> dict[str, str]:
    found: dict[str, str] = {}
    for source in sorted(tree.rglob("*")):
        if source.suffix not in (".ts", ".tsx"):
            continue
        for path in extract(source.read_text()):
            found.setdefault(path, source.relative_to(ROOT).as_posix())
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

    # Extraction has its own failure mode: a truncated literal looks exactly like a missing route.
    source = 'request<X>(`/vendor/audit.csv${qs ? `?${qs}` : \'\'}`); const nav = "/settings";'
    if app_literals(source) != ["/vendor/audit.csv${qs ? `?${qs}` : ''}"]:
        print("self-test failed — app_literals lost a nested template, or picked up navigation")
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

    staff = {p: s for p, s in routes.items() if p.startswith(("/vendor", "/ops"))}
    customer = {p: s for p, s in routes.items() if not p.startswith(("/vendor", "/ops"))}

    problems: list[str] = []

    portal = literals(PORTAL, portal_literals)
    if not portal:
        problems.append("no portal paths found — the scan is looking in the wrong place")
    for path, source in sorted(portal.items()):
        if not any(matches(path, route) for route in staff):
            problems.append(f"portal asks for {path}   ({source})")

    app = literals(APP, app_literals)
    if not app:
        problems.append("no app request paths found — the scan is looking in the wrong place")
    for path, source in sorted(app.items()):
        if not any(matches(path, route) for route in customer):
            problems.append(f"app asks for {path}   ({source})")

    if problems:
        print("paths a client asks for that the server does not serve:")
        for problem in problems:
            print(f"  {problem}")
        return 1

    print(
        f"{len(portal)} portal paths match {len(staff)} staff routes; "
        f"{len(app)} app paths match {len(customer)} customer routes"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
