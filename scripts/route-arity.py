#!/usr/bin/env python3
"""Every route must provide exactly the path parameters its handler extracts.

Why this exists
---------------
`DELETE /notifications/tokens` was declared with no parameter while its handler extracted
`Path<String>`. In axum that is not a compile error — it is a 500 at request time, carrying
axum's own wording in the response body. The app called
`DELETE /notifications/tokens/{token}`, got 404, and unregistering a phone on sign-out had
never worked. Every test passed: the app's tests mock the transport, and no server test called
the route it served. One request found it.

That defect has a shape, so it can be checked mechanically:

    route provides {n} path values   ==   handler extracts {n} path values

Resolution is per module, not global: `list` and `create` exist in several route files with
different signatures, and a single name-keyed map silently pairs a route with the wrong handler
(it produced six false alarms the first time this was written). A qualified handler
(`routes::history::history`) resolves to the file it names.

`--self-test` runs the same logic against a known-bad snippet and fails if it stops finding it,
so a checker that quietly goes blind is itself a red gate.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "backend/crates/api/src"
SOURCES = [ROOT / "app.rs", *sorted((ROOT / "routes").glob("*.rs"))]

BAD_FIXTURE = '''
fn router() -> Router {
    Router::new().route("/things", post(create).delete(remove))
}
async fn create() {}
async fn remove(Path(id): Path<String>) {}
'''


def block_after(text: str, open_paren: int) -> str:
    """The parenthesised argument list starting at `open_paren` (a '(' index)."""
    depth, i = 1, open_paren + 1
    while i < len(text) and depth:
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
        i += 1
    return text[open_paren + 1 : i]


def handlers(text: str) -> dict[str, str]:
    """Function name -> parameter list, for one file."""
    out: dict[str, str] = {}
    for m in re.finditer(r"(?:async\s+)?fn\s+([a-z_0-9]+)\s*(?:<[^>]*>)?\s*\(", text):
        out[m.group(1)] = block_after(text, m.end() - 1)
    return out


def path_arity(params: str) -> int:
    """How many values a handler takes out of the path.

    `Path<T>` is one, `Path<(A, B)>` is two, and `Option<Path<T>>` still consumes one.
    """
    m = re.search(r"Path\s*<\s*(.+?)\s*>(?:\s*,\s*|\))", params, re.S)
    if not m:
        return 0
    inner = m.group(1).strip()
    if inner.startswith("("):
        return max(1, len([p for p in inner.strip("()").split(",") if p.strip()]))
    return 1


def routes(text: str):
    """(method, path, handler as written) for every `.route(...)` in the file."""
    for m in re.finditer(r'\.route\(\s*"([^"]+)"\s*,', text):
        path = m.group(1)
        body = block_after(text, m.end() - 1)
        for method, handler in re.findall(
            r"\b(get|post|put|patch|delete)\(\s*([a-zA-Z_:0-9]+)\s*\)", body
        ):
            yield method.upper(), path, handler


def check_source(
    name: str, text: str, by_module: dict[str, dict[str, str]]
) -> tuple[list[str], int]:
    """Problems in one file, and how many pairs were actually checked."""
    own = by_module.get(name, {})
    problems: list[str] = []
    checked = 0
    for method, path, handler in routes(text):
        if "::" in handler:
            module, fn = handler.split("::")[-2:]
            params = by_module.get(module, {}).get(fn)
        else:
            fn, params = handler, own.get(handler)
        if params is None:
            continue  # imported or generated; not a handler this sweep can see
        provided = len(re.findall(r"\{[^}]+\}", path))
        extracted = path_arity(params)
        checked += 1
        if provided != extracted:
            problems.append(
                f"{name}: {method} {path} -> {handler}() provides {provided} path value(s), "
                f"handler extracts {extracted}"
            )
    return problems, checked


def sweep() -> int:
    by_module: dict[str, dict[str, str]] = {}
    texts: dict[str, str] = {}
    for src in SOURCES:
        if not src.exists():
            print(f"route-arity: missing {src}", file=sys.stderr)
            return 2
        text = src.read_text()
        texts[src.stem] = text
        by_module[src.stem] = handlers(text)

    problems: list[str] = []
    checked = 0
    for name, text in texts.items():
        found, count = check_source(name, text, by_module)
        problems += found
        checked += count

    if problems:
        print("route-arity: a handler and its route disagree about the path\n", file=sys.stderr)
        for p in problems:
            print("  " + p, file=sys.stderr)
        print(
            "\nIn axum this is a 500 at request time, not a compile error. Fix the route or the handler.",
            file=sys.stderr,
        )
        return 1
    print(f"route-arity: {checked} route/handler pairs agree about their path parameters")
    return 0


def self_test() -> int:
    found, _ = check_source("<fixture>", BAD_FIXTURE, {"<fixture>": handlers(BAD_FIXTURE)})
    if len(found) != 1:
        print(
            "route-arity --self-test: the checker no longer flags a route whose handler extracts a "
            f"path value the route does not provide (found {len(found)}). "
            "A blind checker is worse than none.",
            file=sys.stderr,
        )
        return 1
    print("route-arity --self-test: flags the known-bad shape and passes the current tree")
    return sweep()


if __name__ == "__main__":
    sys.exit(self_test() if "--self-test" in sys.argv else sweep())
