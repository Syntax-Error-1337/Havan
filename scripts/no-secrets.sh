#!/usr/bin/env bash
# Fail if a secret or a private key is tracked by git.
#
# This exists because one was: `.haven/ca/ca.key`, the key that signs device certificates, sat in
# the repository and the broker trusted it. Anyone with a clone could mint a certificate for any
# device on any tenant. It was found by a design tool's "no secrets in the repository" foundation
# check, which is an embarrassing way to find out.
#
# The check reads `git ls-files`, so it only ever looks at what would actually be published —
# untracked working files are the developer's business, tracked ones are everyone's.
set -euo pipefail

cd "$(dirname "$0")/.."

# Deliberate fixtures, each with the reason it is safe. A path here is a claim that the material
# signs nothing real; adding one needs that sentence.
ALLOWED_FILES=(
  # A throwaway P-256 key generated for one test, asserted on but never accepted by Apple.
  "backend/crates/worker/src/push.rs"
)

fail=0
report() {
  printf '  %s\n' "$1"
  fail=1
}

echo "secrets: checking tracked files"

# 1. Private key material of any kind.
while IFS= read -r file; do
  [ -f "$file" ] || continue
  if ! grep -qE -- "-----BEGIN (RSA |EC |OPENSSH |PGP )?PRIVATE KEY-----" "$file" 2>/dev/null; then
    continue
  fi
  skip=0
  for allowed in "${ALLOWED_FILES[@]}"; do
    [ "$file" = "$allowed" ] && skip=1
  done
  if [ "$skip" = 1 ]; then
    echo "  ok   $file (documented test fixture)"
  else
    report "PRIVATE KEY in $file"
  fi
done < <(git ls-files)

# 2. Certificate authorities and key directories belong to a deployment, not to a repository.
while IFS= read -r file; do
  case "$file" in
    .haven/*|*/.haven/*) report "deployment CA committed: $file" ;;
  esac
done < <(git ls-files)

# 3. The dev dotenv, by name.
while IFS= read -r file; do
  case "$file" in
    .env|*/.env) report "dotenv committed: $file" ;;
  esac
done < <(git ls-files)

if [ "$fail" = 1 ]; then
  cat >&2 <<'EOF'

A secret is tracked. Remove it from the index and ignore it:

  git rm --cached <path> && printf '%s\n' '<path>' >> .gitignore

Removing the file does not remove it from history. If it was a real key, treat it as published:
rotate it. For the device CA that means deleting .haven/ca, letting the API regenerate it, and
re-enrolling devices — their certificates were signed by the key that leaked.
EOF
  exit 1
fi

echo "secrets: no key material or dotenv is tracked"
