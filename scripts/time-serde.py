#!/usr/bin/env python3
"""Every timestamp a client receives is an RFC 3339 string, not a field tuple.

`time::OffsetDateTime` serialises as `[2026, 321, 3, 26, 34, 97910000, 0, 0]` unless told otherwise.
That array reaches JavaScript, `new Date(array)` produces `Invalid Date`, and the app renders
"just now" for a device that has been silent for four hours — a wrong answer that looks like a
right one, which is the worst kind.

The rule: inside any struct that derives `Serialize`, a `pub` field of type `OffsetDateTime`
(or `Option<...>`) must carry `#[serde(with = "time::serde::rfc3339")]` — `::option` for the
`Option` form. Attributes may sit above a doc comment; the check looks back through comments.

Exit 1 and list the offenders when the rule is broken.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOMAIN = ROOT / "backend" / "crates" / "domain" / "src"

FIELD = re.compile(r"^\s*pub\s+(\w+)\s*:\s*(Option<)?\s*(?:time::)?OffsetDateTime")
STRUCT = re.compile(r"^\s*pub\s+struct\s+(\w+)")
DERIVE = re.compile(r"^\s*#\[derive\((?P<list>[^)]*)\)")


def offending(path: Path) -> list[tuple[int, str, str]]:
    lines = path.read_text().splitlines()
    findings: list[tuple[int, str, str]] = []
    derives_serialize = False

    for index, line in enumerate(lines):
        derive = DERIVE.match(line)
        if derive:
            derives_serialize = "Serialize" in derive.group("list")
            continue

        if STRUCT.match(line):
            continue

        match = FIELD.match(line)
        if not match or not derives_serialize:
            continue

        # Walk back over the field's own attributes and doc comments to find a serde(with = ...).
        cursor = index - 1
        covered = False
        while cursor >= 0:
            above = lines[cursor].strip()
            if above.startswith("#[serde("):
                covered = "time::serde::rfc3339" in above
                break
            if above.startswith("///") or above.startswith("#[") or above == "":
                cursor -= 1
                continue
            break

        if not covered:
            optional = bool(match.group(2))
            expected = (
                '#[serde(with = "time::serde::rfc3339::option")]'
                if optional
                else '#[serde(with = "time::serde::rfc3339")]'
            )
            findings.append((index + 1, line.strip(), expected))

    return findings


def main() -> int:
    total = 0
    for path in sorted(DOMAIN.rglob("*.rs")):
        for line_no, text, expected in offending(path):
            total += 1
            print(f"{path.relative_to(ROOT)}:{line_no}: {text}")
            print(f"    add {expected} above it")

    if total:
        print(
            f"\ntime-serde: {total} timestamp(s) would reach a client as a field tuple.\n"
            "A JS client cannot parse that, and shows 'just now' for a value that may be days old.",
            file=sys.stderr,
        )
        return 1

    print("time-serde: every timestamp a client receives is RFC 3339")
    return 0


if __name__ == "__main__":
    sys.exit(main())
