#!/usr/bin/env bash
# Baseline numbers for the read paths a phone actually calls.
#
# It builds the shape a real home has rather than measuring a clean database: a
# hundred devices in one home and thirty days of history on the device being
# charted. Seeds a fresh bench home each run (SQL, because the point is to measure
# reads, not pairing) and prints min / average / bytes per endpoint.
#
#   ./scripts/bench.sh            # against http://127.0.0.1:8080
#   RUNS=10 ./scripts/bench.sh
#
# Needs `make up`, `make migrate`, `make seed`, `make api`. Clean up with
# `make db-reset` — the bench home is real data until then.
set -euo pipefail

API="${API:-http://127.0.0.1:8080}"
V1="$API/v1"
RUNS="${RUNS:-5}"
TS="$(date +%s)"
EMAIL="bench-$TS@haven.dev"
SECRET="bench-Password-$TS"
VENDOR="11111111-1111-7111-8111-111111111111"
PRODUCT_KEY="${PRODUCT_KEY:-hv_acme_bulb1}"

field() {
  python3 -c 'import sys,json;d=json.load(sys.stdin);k=sys.argv[1];print(d[k] if isinstance(d,dict) else d[0][k])' "$1"
}

echo "benchmarking $API (${RUNS} runs per endpoint)"

# A real account and a real home: the read paths run through the same RLS policies
# a customer's request does, which a direct SQL fixture would skip.
TOKEN="$(curl -s -X POST "$V1/auth/register" -H 'content-type: application/json' \
  -d "{\"email\":\"$EMAIL\",\"password\":\"$SECRET\"}" | field access)"
HOME_ID="$(curl -s -X POST "$V1/homes" -H "authorization: Bearer $TOKEN" \
  -H 'content-type: application/json' -d '{"name":"Bench flat","timezone":"UTC"}' | field id)"

# A hundred devices, three points each, straight into the tables: pairing a hundred
# devices over MQTT would take longer than the measurement is worth.
docker exec -i haven-db psql -U haven -d haven -q -v ON_ERROR_STOP=1 >/dev/null <<SQL
DO \$\$
DECLARE
  i int;
  d uuid;
  prod uuid;
BEGIN
  SELECT id INTO prod FROM products WHERE product_key = '$PRODUCT_KEY';
  IF prod IS NULL THEN
    RAISE EXCEPTION 'product $PRODUCT_KEY is missing — run `make seed` first';
  END IF;

  FOR i IN 1..100 LOOP
    d := gen_random_uuid();
    INSERT INTO devices (id, vendor_id, product_id, product_key, home_id, name, serial,
                         secret_hash, state, category, online, fw_version)
    VALUES (d, '$VENDOR', prod, '$PRODUCT_KEY', '$HOME_ID',
            'Load bulb ' || i, 'BENCH-$TS-' || i, 'x', 'claimed', 'light', (i % 2 = 0), '1.0.0');

    INSERT INTO device_points (id, device_id, vendor_id, capability, slot, code, dp, kind,
                               writable, unit, min_val, max_val, history, value_bool)
    VALUES (gen_random_uuid(), d, '$VENDOR', 'switch', 'power', 'switch.power', '1', 'bool',
            true, NULL, NULL, NULL, 'change', (i % 2 = 0));

    INSERT INTO device_points (id, device_id, vendor_id, capability, slot, code, dp, kind,
                               writable, unit, min_val, max_val, history, value_num)
    VALUES (gen_random_uuid(), d, '$VENDOR', 'brightness', 'level', 'brightness.level', '2', 'int',
            true, '%', 1, 100, 'change', 50);

    INSERT INTO device_points (id, device_id, vendor_id, capability, slot, code, dp, kind,
                               writable, unit, min_val, max_val, history, value_num)
    VALUES (gen_random_uuid(), d, '$VENDOR', 'color_temp', 'kelvin', 'color_temp.kelvin', '3', 'int',
            true, 'K', 2000, 6500, 'interval:300', 4000);
  END LOOP;
END \$\$;
SQL

DEVICE_ID="$(docker exec haven-db psql -U haven -d haven -tAc \
  "SELECT id FROM devices WHERE home_id='$HOME_ID' ORDER BY name LIMIT 1;")"

# Thirty days at one reading every five minutes: 8 640 rows for the charted device.
docker exec -i haven-db psql -U haven -d haven -q -v ON_ERROR_STOP=1 >/dev/null <<SQL
INSERT INTO telemetry (ts, device_id, point_id, vendor_id, num)
SELECT now() - (g || ' minutes')::interval * 5, '$DEVICE_ID',
       (SELECT id FROM device_points WHERE device_id = '$DEVICE_ID' AND code = 'brightness.level'),
       '$VENDOR', 30 + (g % 50)
  FROM generate_series(1, 8640) g;
SQL

echo "home $HOME_ID: $(docker exec haven-db psql -U haven -d haven -tAc "SELECT count(*) FROM devices WHERE home_id='$HOME_ID';") devices, $(docker exec haven-db psql -U haven -d haven -tAc "SELECT count(*) FROM telemetry WHERE device_id='$DEVICE_ID';") telemetry rows"
echo

bench() {
  local name="$1" url="$2" i out times best total bytes=0 file
  file="$(mktemp)"
  times=""
  for ((i = 0; i < RUNS; i++)); do
    out="$(curl -s -o "$file" -w '%{time_total}' "$url" -H "authorization: Bearer $TOKEN")"
    bytes="$(wc -c < "$file" | tr -d ' ')"
    times="$times $out"
  done
  rm -f "$file"
  read -r best total < <(printf '%s\n' $times | awk '
    { if (NR == 1 || $1 < min) min = $1; sum += $1 }
    END { printf "%.4f %.4f\n", min, sum / NR }')
  printf '  %-30s min %6ss  avg %6ss  %9s bytes\n' "$name" "$best" "$total" "$bytes"
}

bench "devices?home_id (100)"     "$V1/devices?home_id=$HOME_ID"
bench "devices/{id}"              "$V1/devices/$DEVICE_ID"
bench "history 1d"                "$V1/devices/$DEVICE_ID/history?code=brightness.level&range=1d"
bench "history 30d"               "$V1/devices/$DEVICE_ID/history?code=brightness.level&range=30d"
bench "notifications unread"      "$V1/notifications/unread-count"
