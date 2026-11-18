#!/usr/bin/env python3
"""Who touches each route: a Rust test, a shell drill, a client, or nobody.

Why this exists
---------------
`make check` proves the routes compile and that their path parameters match their handlers. It says nothing about whether
anyone has ever called them. The e2e and chaos drills live in shell scripts, the clients in TypeScript, and the
integration tests in Rust — so "is this endpoint tested?" has three possible answers and no single place that knows.

This prints the map. It does not fail a build: a route reached only by a client is not a defect, and a route reached by
nothing is a question for a human ("is this an internal hook, or a surface we shipped untested?"). The one thing it will
not do is stay quiet about a route nothing mentions.

Matching is by the route's last static segments — `/homes/{id}/scenes` is looked for as `/scenes` — because every caller
builds the path with interpolated ids and matching literals would report almost everything as uncovered.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ROUTE_LITERAL = re.compile(r'\.route\(\s*"([^"]+)"', re.S)


def routes() -> dict[str, str]:
    found: dict[str, str] = {}
    for source in sorted((ROOT / "backend" / "crates" / "api" / "src").rglob("*.rs")):
        for path in ROUTE_LITERAL.findall(source.read_text()):
            found.setdefault(path, source.name)
    return found


def sources() -> dict[str, str]:
    """Everything that could call a route, in one blob each — the answer is about presence, not frequency."""
    blobs = {}

    def read(folder: Path, pattern: str) -> str:
        return "\n".join(
            p.read_text() for p in sorted(folder.rglob(pattern)) if "node_modules" not in str(p)
        )

    blobs["rust tests"] = read(ROOT / "backend" / "crates" / "api" / "tests", "*.rs")
    # The drills are shell *and* Python: `conformance.py` drives two of the operator routes, and a scan that only read
    # `.sh` reported them as reached by nothing.
    blobs["drills"] = "\n".join(
        p.read_text() for p in sorted((ROOT / "scripts").iterdir()) if p.suffix in (".sh", ".py")
    )
    # The broker calls the authorize hook from its own configuration, which is a caller as real as a client.
    blobs["deploy"] = "\n".join(
        # Text only, and forgivingly: `deploy/certs` holds DER/PEM material, and a coverage map is not a reason to
        # fail on a binary file.
        p.read_text(errors="ignore")
        for p in sorted((ROOT / "deploy").rglob("*"))
        if p.is_file() and p.suffix in ("", ".conf", ".yml", ".yaml", ".sql", ".json", ".sh")
    )
    blobs["app"] = read(ROOT / "app" / "src", "*.ts*")
    blobs["portal"] = read(ROOT / "portal" / "src", "*.ts*")
    return blobs


def tails(path: str) -> list[str]:
    segments = [part for part in path.strip("/").split("/") if part]
    static = [part for part in segments if not part.startswith("{")]
    keys = []
    if static:
        keys.append("/" + static[-1])
    if len(static) > 1:
        keys.append("/" + static[-2] + "/" + static[-1])
    return keys


def main() -> int:
    if "--check" in sys.argv:
        # The gate form: fail only if a route is mentioned by nothing at all.
        blobs = sources()
        orphans = [
            path
            for path in routes()
            if not any(key in blob for key in tails(path) for blob in blobs.values())
        ]
        if orphans:
            print("routes nothing in this repository mentions:", *sorted(orphans), sep="\n  ", end="\n")
            return 1
        print("every route is reached by a test, a drill or a client")
        return 0

    blobs = sources()
    rows = []
    for path, source in sorted(routes().items()):
        where = [name for name, blob in blobs.items() if any(k in blob for k in tails(path))]
        rows.append((path, source, where))

    orphans = [row for row in rows if not row[2]]
    width = max(len(row[0]) for row in rows)
    for path, source, where in rows:
        label = ", ".join(where) if where else "NOTHING"
        print(f"  {path.ljust(width)}  {label:32}  {source}")

    print()
    print(f"{len(rows)} routes, {len(rows) - len(orphans)} reached by something, {len(orphans)} by nothing")
    return 0


if __name__ == "__main__":
    sys.exit(main())
