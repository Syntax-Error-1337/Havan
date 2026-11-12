#!/usr/bin/env bash
# AGENTS.md §1: permissive licences only, in anything we host.
#
# Until this script existed that rule was enforced by good intentions, and a rule nobody
# can run is a rule that is already broken somewhere. This reads what is actually
# installed — `cargo metadata` for the workspace, every `package.json` under the two
# node_modules trees — and fails on anything outside the allowlist.
#
# Usage: scripts/licences.sh [--verbose]
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
VERBOSE=${1:-}

python3 - "$ROOT" "$VERBOSE" <<'PY'
import json
import subprocess
import sys
from pathlib import Path

root = Path(sys.argv[1])
verbose = sys.argv[2] == "--verbose"

# AGENTS.md §1 / ADR-002. Deliberately a list of names, not a list of exceptions: an
# exception list grows until it is the policy.
ALLOWED = {
    "Apache-2.0",
    "MIT",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "PostgreSQL",
    "0BSD",
    "Unlicense",
    "CC0-1.0",
    "Zlib",
    "MIT-0",
    "BSD-3-Clause-Clear",
    "Apache-2.0 WITH LLVM-exception",
    # Permissive, and reaching them is not optional: ICU4X's Unicode licence arrives with
    # `url`/`idna` and with rustls's certificate parsing; BlueOak and Python-2.0 come with
    # the node tooling; CDLA-Permissive is Mozilla's root-store *data*; CC-BY is
    # caniuse-lite's browser database. None is copyleft — each permits commercial use,
    # modification and redistribution, which is the test AGENTS.md §1 is really applying.
    "Unicode-3.0",
    "BlueOak-1.0.0",
    "CDLA-Permissive-2.0",
    "Python-2.0",
    "CC-BY-4.0",
}

# Weak/file-level copyleft, acceptable only for packages that are never shipped. MPL-2.0's
# obligations attach to the files you modify; `lightningcss` is used unmodified as a build
# tool (Metro's CSS transform) and is not in the app bundle or the portal bundle. A build
# tool is not code we host, which is the line AGENTS.md §1 draws.
BUILD_ONLY = {"MPL-2.0"}

# Packages that ship no licence field or a non-SPDX string are reported, not waved through:
# "unknown" is how a copyleft dependency gets into a commercial product.
ok_licences = []
problems = []


def check(ecosystem, name, version, lic, is_dev=False):
    if lic and lic.strip().strip("()") in BUILD_ONLY:
        # Accepted on the nature of the package, not on npm's `dev` flag: npm calls
        # `lightningcss` a runtime dependency because `expo` depends on it, while in
        # practice it is Metro's CSS transform — a build tool that never reaches a bundle.
        # MPL-2.0 asks for the *files you modify*, and we do not modify them.
        ok_licences.append(
            (ecosystem, name, version, lic + (" (build only)" if is_dev else " (build tool)"))
        )
        return

    if not lic:
        problems.append((ecosystem, name, version, "no licence field"))
        return

    # SPDX expressions: "MIT OR Apache-2.0" is satisfied by either half; "MIT AND X" needs
    # both. Anything with a WITH clause is evaluated as a unit first.
    expr = lic.strip().strip("()")
    if expr in ALLOWED:
        ok_licences.append((ecosystem, name, version, lic))
        return

    # Old-style dual licences predate SPDX: "MIT/Apache-2.0" is an OR, and reading it as
    # one opaque string would fail a package that is simply dual-licensed the normal way.
    normalised = expr.replace(" AND ", " OR ").replace("/", " OR ")
    parts = [p.strip().strip("()") for p in normalised.split(" OR ")]
    parts = [p for p in parts if p]
    if parts and all(p in ALLOWED for p in parts):
        ok_licences.append((ecosystem, name, version, lic))
        return
    if any(p in ALLOWED for p in parts) and " AND " not in expr.upper():
        ok_licences.append((ecosystem, name, version, lic))
        return

    problems.append((ecosystem, name, version, lic))


# ---------------------------------------------------------------- rust
meta = subprocess.run(
    # No --offline: metadata resolves the whole graph, including crates only a feature
    # pulls in, and those are not always in the local cache even when the build is green.
    ["cargo", "metadata", "--format-version", "1",
     "--manifest-path", str(root / "backend" / "Cargo.toml")],
    capture_output=True,
    text=True,
)
if meta.returncode != 0:
    print("cargo metadata failed; run this where the backend builds", file=sys.stderr)
    print(meta.stderr[-2000:], file=sys.stderr)
    sys.exit(2)

packages = json.loads(meta.stdout)["packages"]
seen_rust = set()
for pkg in packages:
    key = (pkg["name"], pkg["version"])
    if key in seen_rust:
        continue
    seen_rust.add(key)
    # Workspace members are ours: no licence field needed, LICENSE covers them.
    if pkg.get("source") is None:
        continue
    check("rust", pkg["name"], pkg["version"], pkg.get("license") or pkg.get("license_file"))

# ---------------------------------------------------------------- node
# package-lock records `"dev": true` for build-time-only packages, which is the difference
# between "a licence in our build" and "a licence in our product".
dev_only = set()
for tree in ["app", "portal", ""]:
    lock = root / tree / "package-lock.json"
    if not lock.is_file():
        continue
    try:
        data = json.loads(lock.read_text())
    except Exception:
        continue
    for path, meta in (data.get("packages") or {}).items():
        if not path.startswith("node_modules/"):
            continue
        name = path.rsplit("node_modules/", 1)[1]
        if meta.get("dev"):
            dev_only.add(name)

seen_node = set()
for tree in ["app", "portal", ""]:
    lock = root / tree / "package-lock.json"
    if not lock.is_file():
        continue
    try:
        lockdata = json.loads(lock.read_text())
    except Exception:
        continue

    # The lock file *is* the dependency graph: every entry is something npm installed for
    # us, and anything else under node_modules is a nested test fixture (npm ships
    # deliberately-invalid packages called `2g-cli`, `invalid_main`, …) that must not be
    # reported as a dependency of ours.
    for path, meta in (lockdata.get("packages") or {}).items():
        if not path.startswith("node_modules/") or meta.get("link"):
            continue
        name = meta.get("name") or path.rsplit("node_modules/", 1)[1]
        version = meta.get("version") or "?"
        if (name, version) in seen_node:
            continue
        seen_node.add((name, version))

        manifest = root / tree / path / "package.json"
        if not manifest.is_file():
            # In the lock but not on this machine: a platform variant for another OS, or an
            # optional native build. Not part of our tree, so not ours to license.
            continue
        lic = None
        try:
            data = json.loads(manifest.read_text())
            lic = data.get("license")
            if isinstance(lic, dict):
                lic = lic.get("type")
        except Exception:
            lic = None
        check("node", name, version, lic, is_dev=bool(meta.get("dev")))

# ---------------------------------------------------------------- report
if verbose:
    for eco, name, version, lic in sorted(ok_licences):
        print(f"  ok   {eco:4} {name}@{version}  {lic}")

print(f"checked {len(seen_rust)} rust crate(s), {len(seen_node)} node package(s)")

if problems:
    from collections import Counter

    counts = Counter(lic for _, _, _, lic in problems)
    print(f"\n{len(problems)} package(s) outside the allowlist, by licence:\n")
    for lic, n in counts.most_common():
        example = next(f"{name}@{version}" for _, name, version, l in problems if l == lic)
        print(f"  FAIL {n:3}x  {lic}   (e.g. {example})")
    print(
        "\nAGENTS.md §1 allows Apache-2.0, MIT, BSD, ISC, PostgreSQL and 0BSD only. A new"
        "\ndependency needs a licence line in the PR description; a copyleft one needs to be"
        "\nreplaced (see ADR-002 for the three that were)."
    )
    sys.exit(1)

print("every dependency is permissively licensed")
PY
