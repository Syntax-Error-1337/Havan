#!/usr/bin/env bash
# A backup that has been verified, not just written.
#
# The failure this is built to avoid: a dump that is truncated, empty, or a zero-byte file
# left behind by a full disk — which looks exactly like a backup until the day it is
# needed. So the dump is written to `.partial`, `pg_restore --list` must report a table of
# contents, and only then is it renamed into the set. A file in the set is a file that
# parsed.
#
# Usage: scripts/backup.sh [output-dir]   (default: .haven/backups)
#   BACKUP_KEEP=7   how many dumps to keep
#   DATABASE_URL    required, as anywhere else
set -euo pipefail

cd "$(dirname "$0")/.."

dest=${1:-.haven/backups}
keep=${BACKUP_KEEP:-7}
stamp=$(date -u +%Y%m%dT%H%M%SZ)
file="$dest/haven-$stamp.dump"
partial="$file.partial"

: "${DATABASE_URL:?DATABASE_URL is not set}"
case "$DATABASE_URL" in
  postgres://*|postgresql://*) ;;
  *) echo "backup: DATABASE_URL does not look like a Postgres URL" >&2; exit 2 ;;
esac

mkdir -p "$dest"

# Prefer a local pg_dump so the dump is produced by the client the operator has; fall back
# to the compose container, which is where the server lives in development. Which one ran
# is printed, because "restored from a dump made by a different major version" is a
# failure worth seeing before it happens.
if command -v pg_dump >/dev/null 2>&1; then
  dump() { pg_dump --format=custom --no-owner --no-privileges --file="$1" "$DATABASE_URL"; }
  source_desc="local pg_dump"
elif docker ps --format '{{.Names}}' 2>/dev/null | grep -qx 'haven-db'; then
  # Inside the container the URL's host and port point at the host, so the container's own
  # socket and role are used instead.
  dump() {
    docker exec haven-db pg_dump -U "${PGUSER:-haven}" -d "${PGDATABASE:-haven}" \
      --format=custom --no-owner --no-privileges >"$1"
  }
  source_desc="docker exec haven-db"
else
  echo "backup: no pg_dump on PATH and no haven-db container to use" >&2
  exit 3
fi

echo "backup: dumping via $source_desc -> $file"
trap 'rm -f "$partial"' EXIT
dump "$partial"

if [ ! -s "$partial" ]; then
  echo "backup: the dump is empty, so it is not a backup" >&2
  exit 4
fi

# The only check that separates a dump from a file: can a restore tool read its catalogue?
if command -v pg_restore >/dev/null 2>&1; then
  pg_restore --list "$partial" >/dev/null
  entries=$(pg_restore --list "$partial" | grep -c 'TABLE DATA' || true)
else
  entries=$(docker exec -i haven-db pg_restore --list <"$partial" | grep -c 'TABLE DATA' || true)
fi

if [ "${entries:-0}" -lt 10 ]; then
  echo "backup: the dump lists only $entries tables of data; refusing to call that a backup" >&2
  exit 5
fi

mv "$partial" "$file"
trap - EXIT

size=$(du -h "$file" | cut -f1)
echo "backup: ok — $file ($size, $entries tables of data)"

# Retention by count, oldest first. A backup directory that grows forever is a disk that
# fills and takes the database's own WAL with it. Deliberately not `mapfile`, which is bash
# 4: macOS still ships bash 3.2, and a backup script that dies on the operator's laptop is
# the one script that has to run everywhere.
ls -1t "$dest"/haven-*.dump 2>/dev/null | tail -n +$((keep + 1)) | while read -r f; do
  [ -n "$f" ] || continue
  rm -f "$f"
  echo "backup: pruned $(basename "$f") (keeping $keep)"
done
