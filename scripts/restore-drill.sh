#!/usr/bin/env bash
# Restore the newest dump into a throwaway database and prove it is the same database.
#
# A backup nobody has restored is a hypothesis. This runs the hypothesis on a schedule a
# human can afford — it creates `haven_restore_drill`, restores into it, compares per-table
# row counts and the migration count against the live database, and drops it whether or not
# the comparison passed (a drill that leaves a stale database behind is its own incident).
#
# Counts are compared with a tolerance because the live database keeps accepting writes
# while the restore runs: the restored copy can only ever be *behind*, never ahead. A table
# that is ahead of the source is a real failure — it means the dump came from a different
# database than the one being compared.
#
# Usage: scripts/restore-drill.sh [dump-file]
#   DRILL_TOLERANCE=0.05   fraction of rows a table may be behind
set -euo pipefail

cd "$(dirname "$0")/.."

tolerance=${DRILL_TOLERANCE:-0.05}
drill_db=haven_restore_drill
dump=${1:-}

: "${DATABASE_URL:?DATABASE_URL is not set}"
admin_url=$(printf '%s' "$DATABASE_URL" | sed 's#/[^/?]*\(\?.*\)\?$##')

# A psql that can reach the server: local client, or the compose container.
if command -v psql >/dev/null 2>&1; then
  psql_do() { psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -Atc "$1"; }
  psql_db() { psql "${2:-$DATABASE_URL}" -v ON_ERROR_STOP=1 -Atc "$1"; }
  restore_into() { pg_restore --no-owner --no-privileges --dbname="$1" "$2"; }
else
  psql_do() { docker exec haven-db psql -U "${PGUSER:-haven}" -d "${PGDATABASE:-haven}" -v ON_ERROR_STOP=1 -Atc "$1"; }
  psql_db() { docker exec haven-db psql -U "${PGUSER:-haven}" -d "$2" -v ON_ERROR_STOP=1 -Atc "$1"; }
  restore_into() { docker exec -i haven-db pg_restore -U "${PGUSER:-haven}" --no-owner --no-privileges --dbname="$1" <"$2"; }
fi

if [ -z "$dump" ]; then
  dump=$(ls -1t .haven/backups/haven-*.dump 2>/dev/null | head -1 || true)
fi
if [ -z "$dump" ] || [ ! -s "$dump" ]; then
  echo "drill: no dump found — run scripts/backup.sh first" >&2
  exit 2
fi
echo "drill: restoring $(basename "$dump") into $drill_db"

# Start from nothing. A leftover database from a failed drill would make the comparison
# pass against yesterday's data.
psql_db "DROP DATABASE IF EXISTS $drill_db" postgres || true
psql_db "CREATE DATABASE $drill_db" postgres

cleanup() {
  psql_db "DROP DATABASE IF EXISTS $drill_db" postgres >/dev/null 2>&1 || true
}
trap cleanup EXIT

# `pg_restore` exits non-zero for warnings it is safe to ignore when the target starts empty
# (missing roles, comments on extensions). The comparison below is what decides, not the
# exit code, so this is allowed to continue — but its output is kept.
set +e
restore_out=$(restore_into "$drill_db" "$dump" 2>&1)
restore_code=$?
set -e
if [ $restore_code -ne 0 ]; then
  echo "drill: pg_restore exited $restore_code; checking the data anyway"
  printf '%s\n' "$restore_out" | tail -5
fi

# The comparison set: the tables where losing rows changes what the product does, plus the
# licence ledger, because a billing reconciliation is the first thing anyone asks for.
tables=(
  vendors vendor_users products product_serials devices device_points commands
  homes home_members scenes automations audit_log pairing_tickets
  licence_grants licence_activations
)

fail=0
printf '%-22s %10s %10s\n' table source restored
for t in "${tables[@]}"; do
  src=$(psql_do "SELECT count(*) FROM $t" 2>/dev/null || echo "absent")
  dst=$(psql_db "SELECT count(*) FROM $t" "$drill_db" 2>/dev/null || echo "absent")

  if [ "$src" = "absent" ] || [ "$dst" = "absent" ]; then
    printf '%-22s %10s %10s  MISSING\n' "$t" "$src" "$dst"
    fail=1
    continue
  fi

  # Restored may be behind the source; it may never be ahead of it.
  if [ "$dst" -gt "$src" ]; then
    printf '%-22s %10s %10s  AHEAD (wrong database?)\n' "$t" "$src" "$dst"
    fail=1
    continue
  fi
  missing=$((src - dst))
  allowed=$(awk -v s="$src" -v t="$tolerance" 'BEGIN { printf "%d", s * t }')
  if [ "$missing" -gt "$allowed" ]; then
    printf '%-22s %10s %10s  BEHIND by %s\n' "$t" "$src" "$dst" "$missing"
    fail=1
    continue
  fi
  printf '%-22s %10s %10s  ok\n' "$t" "$src" "$dst"
done

# The schema that makes the rows meaningful: a restored database at a different migration is
# a database whose rows mean something else.
src_migrations=$(psql_do "SELECT count(*) FROM _sqlx_migrations" 2>/dev/null || echo 0)
dst_migrations=$(psql_db "SELECT count(*) FROM _sqlx_migrations" "$drill_db" 2>/dev/null || echo 0)
if [ "$src_migrations" != "$dst_migrations" ]; then
  echo "drill: migrations differ — source $src_migrations, restored $dst_migrations"
  fail=1
fi

# A restore that has rows but no recent writes restored a table, not a database.
src_recent=$(psql_do "SELECT COALESCE(max(updated_at)::text, 'none') FROM device_points" 2>/dev/null || echo none)
dst_recent=$(psql_db "SELECT COALESCE(max(updated_at)::text, 'none') FROM device_points" "$drill_db" 2>/dev/null || echo none)
echo "drill: newest device point — source $src_recent, restored $dst_recent"

if [ $fail -ne 0 ]; then
  echo "drill: FAILED — the dump does not reproduce this database"
  exit 1
fi

echo "drill: ok — $drill_db matched the source on ${#tables[@]} tables and $src_migrations migrations"
