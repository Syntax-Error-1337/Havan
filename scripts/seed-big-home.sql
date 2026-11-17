-- A home big enough to page.
--
-- The demo home fits in one dashboard page, which means the paging footer and the windowed list were
-- never actually seen working. This seeds a second home for the demo account with 420 devices across
-- three rooms: page one stops at 200, the footer offers the rest, and the grid has to stay usable.
--
-- Dev fixture only. Idempotent: it recreates the home's devices only when the home is missing, and
-- its serials are prefixed `BIG-` so nothing real is ever touched.

\set vendor '''11111111-1111-7111-8111-111111111111'''

BEGIN;

INSERT INTO homes (id, name, timezone, owner_id)
VALUES ('01a1fc00-0000-7000-8000-000000000001', 'Bench house', 'UTC', (SELECT id FROM users WHERE email = 'demo@haven.local'))
ON CONFLICT (id) DO NOTHING;

INSERT INTO home_members (home_id, user_id, role)
VALUES ('01a1fc00-0000-7000-8000-000000000001', (SELECT id FROM users WHERE email = 'demo@haven.local'), 'owner')
ON CONFLICT DO NOTHING;

INSERT INTO rooms (id, home_id, name, sort)
VALUES
  ('01a1fc01-0000-7000-8000-000000000001', '01a1fc00-0000-7000-8000-000000000001', 'Ground floor', 0),
  ('01a1fc01-0000-7000-8000-000000000002', '01a1fc00-0000-7000-8000-000000000001', 'First floor', 1),
  ('01a1fc01-0000-7000-8000-000000000003', '01a1fc00-0000-7000-8000-000000000001', 'Outbuildings', 2)
ON CONFLICT (id) DO NOTHING;

-- 420 devices. `generate_series` rather than 420 statements, and the room alternates so every page
-- has devices from more than one room — a paging bug that only breaks across a room boundary is the
-- one this is here to catch.
INSERT INTO devices (id, vendor_id, product_id, product_key, category, home_id, room_id, name,
                     serial, secret_hash, state, claimed_at, online, last_seen_at)
SELECT
  ('01a1fd00-0000-7000-8000-' || lpad(to_hex(100000000000 + g), 12, '0'))::uuid,
  :vendor,
  p.id,
  p.product_key,
  p.category,
  '01a1fc00-0000-7000-8000-000000000001',
  CASE g % 3
    WHEN 0 THEN '01a1fc01-0000-7000-8000-000000000001'::uuid
    WHEN 1 THEN '01a1fc01-0000-7000-8000-000000000002'::uuid
    ELSE '01a1fc01-0000-7000-8000-000000000003'::uuid
  END,
  CASE g % 3
    WHEN 0 THEN 'Lamp '
    WHEN 1 THEN 'Plug '
    ELSE 'Sensor '
  END || lpad(g::text, 3, '0'),
  'BIG-' || lpad(g::text, 4, '0'),
  'seeded-not-a-secret',
  'claimed',
  now() - interval '30 days',
  g % 7 <> 0,
  now() - (g % 40) * interval '1 minute'
FROM generate_series(1, 420) AS g
JOIN products p
  ON p.product_key = CASE g % 3
       WHEN 0 THEN 'hv_acme_bulb1'
       WHEN 1 THEN 'hv_acme_plug1'
       ELSE 'hv_acme_sensor1'
     END
WHERE NOT EXISTS (
  SELECT 1 FROM devices d WHERE d.serial = 'BIG-' || lpad(g::text, 4, '0')
);

-- One point per device is enough to prove the page-scoped point query: the payload doubles otherwise.
INSERT INTO device_points (id, device_id, vendor_id, capability, slot, code, dp, kind, writable,
                           unit, min_val, max_val, value_num, value_bool, updated_at)
SELECT gen_random_uuid(), d.id, :vendor,
       CASE WHEN d.category = 'light' THEN 'switch'
            WHEN d.category = 'plug' THEN 'switch'
            ELSE 'temperature' END,
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
WHERE d.home_id = '01a1fc00-0000-7000-8000-000000000001'
  AND NOT EXISTS (SELECT 1 FROM device_points dp WHERE dp.device_id = d.id);

COMMIT;
