#!/usr/bin/env bash
# Where the five minutes go. Times each gate in `make check`, so the next optimisation is chosen by data.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
set -a; . ./.env; set +a

timeit() {
  local label="$1"; shift
  local start end
  start=$(python3 -c 'import time;print(time.time())')
  if "$@" > /tmp/gate.out 2>&1; then status=ok; else status=FAIL; fi
  end=$(python3 -c 'import time;print(time.time())')
  printf '%-34s %6.1fs  %s\n' "$label" "$(python3 -c "print($end-$start)")" "$status"
}

timeit "testdb"            make testdb
timeit "fmt --check"       bash -c "cd backend && cargo fmt --all --check"
timeit "no-secrets"        bash scripts/no-secrets.sh
timeit "route-arity"       python3 scripts/route-arity.py --self-test
timeit "contrast"          make contrast
timeit "tokens"            make tokens
timeit "time-serde"        make time-serde
timeit "clippy --all-targets" bash -c "cd backend && cargo clippy --workspace --all-targets -- -D warnings"
timeit "cargo test"        bash -c "cd backend && cargo test --workspace"
