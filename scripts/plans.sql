-- Query plans for the paths the app actually calls.
--
-- Run against a database that has been shaped like production (see
-- scripts/bench.sh): a hundred devices in one home, thirty days of telemetry on
-- one point. Empty tables answer in 0.4 ms with a seq scan and tell you nothing.
--
--   psql "$DATABASE_URL" -v home_id="'…'" -v device_id="'…'" -v point_id="'…'" \
--        -f scripts/plans.sql

\set ON_ERROR_STOP on

\echo '--- device list (home) ---'
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT d.id, d.name
  FROM devices d
  LEFT JOIN rooms r ON r.id = d.room_id
 WHERE d.home_id = :home_id
 ORDER BY r.sort, d.name;

\echo '--- points for the home (the 99 KB half) ---'
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT dp.code
  FROM device_points dp
  JOIN devices d ON d.id = dp.device_id
 WHERE d.home_id = :home_id;

\echo '--- history, 1 day (the default range) ---'
-- Want: Index Only Scan on (device_id, point_id, ts).
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT count(*)
  FROM telemetry
 WHERE device_id = :device_id
   AND point_id = :point_id
   AND ts >= now() - interval '1 day';

\echo '--- history, 30 days ---'
-- A seq scan here is correct: the window covers nearly every row in the chunk.
EXPLAIN (ANALYZE, BUFFERS, COSTS OFF)
SELECT count(*)
  FROM telemetry
 WHERE device_id = :device_id
   AND point_id = :point_id
   AND ts >= now() - interval '30 days';
