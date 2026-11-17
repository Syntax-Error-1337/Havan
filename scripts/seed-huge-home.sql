-- A home big enough to break the things 420 did not.
--
-- "Bench house" (seed-big-home.sql, 420 devices) proved the dashboard pages instead of rendering
-- everything, and that 48 tiles are enough to fill a phone. 1 200 devices is the next order of magnitude:
-- six pages, a footer that has to say "1 000 more", and a socket event that must patch one device without
-- re-fetching the page it belongs to.
--
-- Dev fixture only. Idempotent: serials are prefixed `HUGE-`, and the home is created only if missing.

\set vendor '''11111111-1111-7111-8111-111111111111'''
\set home '''01a1fd00-0000-7000-8000-0000000000aa'''

BEGIN;

INSERT INTO homes (id, name, timezone, owner_id)
VALUES (:home, 'Plant', 'UTC', (SELECT id FROM users WHERE email = 'demo@haven.local'))
ON CONFLICT (id) DO NOTHING;

INSERT INTO home_members (home_id, user_id, role)
VALUES (:home, (SELECT id FROM users WHERE email = 'demo@haven.local'), 'owner')
ON CONFLICT DO NOTHING;

INSERT INTO rooms (id, home_id, name, sort)
VALUES
  ('01a1fd01-0000-7000-8000-000000000001', :home, 'Line A', 0),
  ('01a1fd01-0000-7000-8000-000000000002', :home, 'Line B', 1),
  ('01a1fd01-0000-7000-8000-000000000003', :home, 'Warehouse', 2)
ON CONFLICT (id) DO NOTHING;

-- 1 200 devices. The room cycles, so any page that only breaks across a room boundary still breaks.
INSERT INTO devices (id, vendor_id, product_id, product_key, category, home_id, room_id, name,
                     serial, secret_hash, state, claimed_at, online, last_seen_at)
SELECT
  ('01a1fd10-0000-7000-8000-' || lpad(to_hex(200000000000 + g), 12, '0'))::uuid,
  :vendor,
  p.id,
  p.product_key,
  p.category,
  :home,
  CASE g % 3
    WHEN 0 THEN '01a1fd01-0000-7000-8000-000000000001'::uuid
    WHEN 1 THEN '01a1fd01-0000-7000-8000-000000000002'::uuid
    ELSE '01a1fd01-0000-7000-8000-000000000003'::uuid
  END,
  CASE g % 3
    WHEN 0 THEN 'Lamp '
    WHEN 1 THEN 'Plug '
    ELSE 'Sensor '
  END || lpad(g::text, 4, '0'),
  'HUGE-' || lpad(g::text, 5, '0'),
  'seeded-not-a-secret',
  'claimed',
  now() - interval '30 days',
  -- Two in three online: a home where everything is up never exercises the offline styling. The
  -- `last_seen_at` of an online device has to be *inside* the presence sweep's 90-second window, or the
  -- worker marks the whole seed offline before anyone looks at it — which is exactly what happened the
  -- first time this fixture ran with the worker up.
  g % 3 <> 0,
  CASE WHEN g % 3 <> 0 THEN now() - (g % 3) * interval '20 seconds'
       ELSE now() - interval '3 days' END
FROM generate_series(1, 1200) AS g
JOIN products p
  ON p.product_key = CASE g % 3
       WHEN 0 THEN 'hv_acme_bulb1'
       WHEN 1 THEN 'hv_acme_plug1'
       ELSE 'hv_acme_sensor1'
     END
WHERE NOT EXISTS (
  SELECT 1 FROM devices d WHERE d.serial = 'HUGE-' || lpad(g::text, 5, '0')
);

INSERT INTO device_points (id, device_id, vendor_id, capability, slot, code, dp, kind, writable,
                           unit, min_val, max_val, value_num, value_bool, updated_at)
SELECT gen_random_uuid(), d.id, :vendor,
       CASE WHEN d.category = 'sensor' THEN 'temperature' ELSE 'switch' END,
       CASE WHEN d.category = 'sensor' THEN 'value' ELSE 'power' END,
       CASE WHEN d.category = 'sensor' THEN 'temperature.value' ELSE 'switch.power' END,
       '1',
       CASE WHEN d.category = 'sensor' THEN 'float' ELSE 'bool' END,
       d.category <> 'sensor',
       CASE WHEN d.category = 'sensor' THEN '°C' ELSE NULL END,
       CASE WHEN d.category = 'sensor' THEN -10 ELSE NULL END,
       CASE WHEN d.category = 'sensor' THEN 50 ELSE NULL END,
       CASE WHEN d.category = 'sensor' THEN 21.0 ELSE NULL END,
       CASE WHEN d.category = 'sensor' THEN NULL ELSE true END,
       now()
FROM devices d
WHERE d.home_id = :home
  AND NOT EXISTS (SELECT 1 FROM device_points dp WHERE dp.device_id = d.id);

COMMIT;
