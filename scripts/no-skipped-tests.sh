#!/usr/bin/env bash
# Run the suite, and fail if any test skipped itself.
#
# Why this exists
# ---------------
# Seven tests in this repository were reporting "TEST_DATABASE_URL not set" with the database configured and every
# service up. The real causes were a row-level-security policy added after the fixtures were written, a fixture that set
# the row scope on the connection pool instead of a connection, a missing owner row now required by a foreign key, and
# three tests sharing one MQTT client id so the broker kept kicking them off each other. None of that was visible: each
# test folded any failure into `None` and turned `None` into a skip, and a skip is green.
#
# A skipped test is a test that did not run. It is indistinguishable from coverage in every report, and it hides the
# reason it stopped working. So the suite runs here, and a skip is a failure with the line that skipped.
#
# The one legitimate skip is a suite with no database at all — a developer running `cargo test` on a laptop without the
# containers. That case is covered by `HAVEN_ALLOW_SKIPS=1`, set by `make check-fast` and never by `make check`.
set -uo pipefail

log="${TMPDIR:-/tmp}/haven-tests.log"

# `--check-only` inspects a log that already exists instead of running the suite, so this guard can be tested without
# waiting for a five-minute build: a checker nobody can test is a checker nobody trusts.
if [ "${1:-}" = "--check-only" ] && [ ! -f "$log" ]; then
  echo "no test log at $log — nothing was checked" >&2
  exit 1
fi

if [ "${1:-}" != "--check-only" ]; then
  : > "$log"
  # pipefail matters: without it the exit status of this pipeline is `tee`'s, which is always zero, and every failure
  # below would be reported as "tests passed".
  if ! cargo test --workspace 2>&1 | tee "$log"; then
    echo "the suite failed; see $log" >&2
    exit 1
  fi
fi

if [ "${HAVEN_ALLOW_SKIPS:-0}" = "1" ]; then
  count=$(grep -ciE "^(skip|skipping)" "$log" || true)
  [ "$count" = "0" ] || echo "note: $count test(s) skipped (allowed here)"
  exit 0
fi

if grep -qiE "^(skip|skipping)" "$log"; then
  echo >&2
  echo "a test skipped itself, so it did not run:" >&2
  grep -inE "^(skip|skipping)" "$log" | sed 's/^/  /' >&2
  echo >&2
  echo "A skip is green, which is why this is red. Fix the fixture, or make the reason explicit and set" >&2
  echo "HAVEN_ALLOW_SKIPS=1 for a run without a database." >&2
  exit 1
fi

echo "no test skipped"
