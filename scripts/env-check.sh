#!/usr/bin/env bash
# Do the credentials in .env agree with each other, and with the broker's config?
#
# Every rule here is an outage this repository has actually had, and every one of them failed
# the same way: the service started, logged something that looked like a network problem, and
# reported healthy enough. None of them printed a secret, and neither does this.
#
# Usage: scripts/env-check.sh     (reads .env; prints only which invariant failed)
set -euo pipefail

cd "$(dirname "$0")/.."

if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi

fail=0
ok() { printf '  ok    %s\n' "$1"; }
bad() {
  printf '  FAIL  %s\n' "$1"
  printf '        %s\n' "$2"
  fail=1
}

echo "env: the platform's own MQTT identity"
# The auth webhook compares the connecting client's password against
# HAVEN_MQTT_SERVICE_TOKEN, so the password the API sends must *be* that token. When they
# drift, the broker answers "not_allowed", closes the connection, and the API logs
# "Connection closed by peer abruptly" forever — which reads like a network fault.
if [ -n "${HAVEN_MQTT_PASS:-}" ] && [ "${HAVEN_MQTT_PASS}" = "${HAVEN_MQTT_SERVICE_TOKEN:-}" ]; then
  ok "HAVEN_MQTT_PASS matches HAVEN_MQTT_SERVICE_TOKEN"
else
  bad "HAVEN_MQTT_PASS and HAVEN_MQTT_SERVICE_TOKEN differ" \
    "the broker compares the client password to the service token; set both to the same value"
fi

# The broker presents this token on its webhook calls; the API validates exactly this string.
conf_token=$(grep -oE 'token=[A-Za-z0-9._~-]+' deploy/vernemq.conf | head -1 | cut -d= -f2)
if [ -n "${HAVEN_MQTT_HOOK_TOKEN:-}" ] && [ "${HAVEN_MQTT_HOOK_TOKEN}" = "$conf_token" ]; then
  ok "HAVEN_MQTT_HOOK_TOKEN matches deploy/vernemq.conf"
else
  bad "HAVEN_MQTT_HOOK_TOKEN does not match the token in deploy/vernemq.conf" \
    "the broker sends the conf's token; the API rejects anything else with not_allowed"
fi

echo "env: secrets the service will not start without"
for name in HAVEN_JWT_SECRET HAVEN_TICKET_SECRET; do
  value=$(eval "printf '%s' \"\${$name:-}\"")
  if [ ${#value} -ge 16 ]; then
    ok "$name is set (${#value} chars)"
  else
    bad "$name is missing or shorter than 16 characters" "make dev-backend needs it"
  fi
done

echo "env: the URLs the services dial"
case "${MQTT_URL:-}" in
  mqtt://*|mqtts://*) ok "MQTT_URL is set" ;;
  *) bad "MQTT_URL is missing" "the API and workers cannot reach the broker" ;;
esac
case "${DATABASE_URL:-}" in
  postgres://*|postgresql://*) ok "DATABASE_URL is set" ;;
  *) bad "DATABASE_URL is missing" "" ;;
esac

if [ $fail -ne 0 ]; then
  echo "env: FAILED"
  exit 1
fi
echo "env: ok"
