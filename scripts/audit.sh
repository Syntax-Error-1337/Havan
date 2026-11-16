#!/usr/bin/env bash
# Dependency vulnerability scan for the two JS workspaces, and the Rust workspace when the tool is
# present. Fails on high or critical advisories; reports moderate ones without failing.
#
# The split is deliberate. Shipped code must be clean at every level, so `--omit=dev` has to come
# back empty. The full tree includes build tooling, where a moderate advisory in an Expo transitive
# dependency cannot be fixed without a breaking toolchain upgrade — failing the build for that
# teaches people to ignore the check, which is the only outcome worse than the advisory.
set -euo pipefail

cd "$(dirname "$0")/.."

fail=0
skipped=0

for dir in portal app; do
  [ -f "$dir/package.json" ] || continue
  echo "audit: $dir (shipped dependencies, high and critical)"
  if ! out=$(cd "$dir" && npm audit --omit=dev --audit-level=high 2>&1); then
    printf '%s\n' "$out" | grep -vE "^npm (notice|warn)" | tail -12
    fail=1
  else
    echo "  ok   no advisories in shipped dependencies"
  fi

  echo "audit: $dir (build tooling, high and critical)"
  if ! out=$(cd "$dir" && npm audit --audit-level=high 2>&1); then
    printf '%s\n' "$out" | grep -vE "^npm (notice|warn)" | tail -12
    fail=1
  else
    # Count what is left so a growing pile of moderates is visible rather than silent.
    summary=$(cd "$dir" && npm audit --json 2>/dev/null | python3 -c 'import json, sys
d = json.load(sys.stdin)
v = d.get("metadata", {}).get("vulnerabilities", {})
parts = ["total", "moderate", "low"]
print(", ".join(str(v.get(p, 0)) + " " + p for p in parts))' 2>/dev/null)
    echo "  ok   no high or critical; tooling tree reports $summary"
  fi
done

echo "audit: rust workspace"
if command -v cargo-audit >/dev/null 2>&1; then
  if ! out=$(cd backend && cargo audit 2>&1); then
    printf '%s\n' "$out" | tail -20
    fail=1
  else
    echo "  ok   no advisories against the RustSec database"
  fi
elif cargo audit --version >/dev/null 2>&1; then
  if ! out=$(cd backend && cargo audit 2>&1); then
    printf '%s\n' "$out" | tail -20
    fail=1
  else
    echo "  ok   no advisories against the RustSec database"
  fi
else
  # Not a silent pass: an unscanned workspace is a different claim from a clean one.
  echo "  skip cargo-audit is not installed; run 'cargo install cargo-audit' to include the Rust crates"
  skipped=1
fi

if [ "$fail" = 1 ]; then
  echo >&2
  echo "audit: high or critical advisories found in shipped code" >&2
  exit 1
fi

if [ "$skipped" = 1 ]; then
  echo "audit: JS workspaces clean up to high; Rust workspace NOT scanned"
else
  echo "audit: no high or critical advisories"
fi
