#!/usr/bin/env bash
# Run the backend suite the way CI runs it: as `haven_app`, a role row-level
# security actually applies to, on a scratch database built from the migrations
# and the deploy's own role bootstrap.
#
# Why this exists: running the suite locally against the dev database connects as
# its owner, which bypasses RLS — so every policy could be wrong and the tests
# would still pass. CI proves the app-role path on every push; this proves it
# before you push.
#
#   ./scripts/ci-backend.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

[ -f .env ] || { echo "no .env; copy .env.example and fill it in" >&2; exit 1; }
set -a; . ./.env; set +a

DB="${CI_DB:-haven_ci}"
CONTAINER="${DB_CONTAINER:-haven-db}"
OWNER_USER="${DB_OWNER_USER:-haven}"

# The owner connection is the migrate one; the scratch database is a sibling.
OWNER_URL="${MIGRATE_DATABASE_URL%/*}/$DB"
APP_URL="postgres://haven_app:haven_app@$(echo "$OWNER_URL" | sed -E 's|.*@([^/]+)/.*|\1|')/$DB"

q() { docker exec -i "$CONTAINER" psql -U "$OWNER_USER" "$@"; }

echo "== scratch database $DB =="
q -d postgres -q -c "DROP DATABASE IF EXISTS $DB;"
q -d postgres -q -c "CREATE DATABASE $DB OWNER $OWNER_USER;"
docker exec -i "$CONTAINER" psql -U "$OWNER_USER" -d "$DB" -q < deploy/postgres/init.sql

echo "== migrations, as the owner =="
cd backend
MIGRATE_DATABASE_URL="$OWNER_URL" DATABASE_URL="$APP_URL" cargo run -q -p migrate

POLICIES=$(docker exec -i "$CONTAINER" psql -U "$OWNER_USER" -d "$DB" -tAc "SELECT count(*) FROM pg_policy")
echo "== row-level policies in the fresh schema: $POLICIES =="
[ "$POLICIES" -gt 0 ] || { echo "no policies: migrations did not apply" >&2; exit 1; }

echo "== the suite, as haven_app =="
set -o pipefail
DATABASE_URL="$APP_URL" \
TEST_DATABASE_URL="$APP_URL" \
MIGRATE_DATABASE_URL="$OWNER_URL" \
REDIS_URL="$REDIS_URL" \
TEST_REDIS_URL="$REDIS_URL" \
MQTT_URL="${MQTT_URL:-mqtt://127.0.0.1:1883}" \
  cargo test --workspace 2>&1 | tee /tmp/ci-backend.log

if grep -q "skipping:" /tmp/ci-backend.log; then
  echo "an integration suite skipped — a service it needs is missing" >&2
  exit 1
fi
