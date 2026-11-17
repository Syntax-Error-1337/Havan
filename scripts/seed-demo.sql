-- Demo seed: a home worth looking at.
--
-- The dev database had one bulb in one unassigned room, which is enough to prove the plumbing and
-- not enough to see the interface: no rooms to group, no second category to colour, no reading with
-- a history behind it. This fills the demo home with the products the vendor catalogue already has,
-- and writes a day of temperature telemetry so the hero card has a shape to draw.
--
-- Idempotent: every insert is guarded by a serial or a name that already exists, so running it twice
-- changes nothing. Safe on a dev database only — it writes to the demo account's home by id.

\set demo_home '''01a1d936-4e1a-7335-964b-11b6c6c170fe'''
\set vendor '''11111111-1111-7111-8111-111111111111'''

BEGIN;

INSERT INTO rooms (id, home_id, name, icon, sort)
VALUES
  ('01a1fa00-0000-7000-8000-000000000001', :demo_home, 'Living Room', 'sofa', 0),
  ('01a1fa00-0000-7000-8000-000000000002', :demo_home, 'Bedroom', 'bed', 1),
  ('01a1fa00-0000-7000-8000-000000000003', :demo_home, 'Kitchen', 'kitchen', 2)
ON CONFLICT (id) DO NOTHING;

-- Devices. Product ids are looked up by key so this does not hard-code a uuid.
INSERT INTO devices (id, vendor_id, product_id, home_id, room_id, name, serial, secret_hash,
                     online, last_seen_at, transport, state, claimed_at, product_key, category)
SELECT v.id, :vendor, p.id, :demo_home, v.room_id, v.name, v.serial,
       'seeded-not-a-secret', v.online, now() - (v.silent_for || ' minutes')::interval,
       'cloud', 'claimed', now() - interval '30 days', p.product_key, p.category
FROM (
  VALUES
    ('01a1fb00-0000-7000-8000-000000000001'::uuid, 'hv_acme_bulb1',   '01a1fa00-0000-7000-8000-000000000001'::uuid, 'Floor Lamp',   'SEED-LIGHT-1',  true,  0),
    ('01a1fb00-0000-7000-8000-000000000002'::uuid, 'hv_acme_sensor1', '01a1fa00-0000-7000-8000-000000000001'::uuid, 'Room Sensor',  'SEED-SENSOR-1', true,  0),
    ('01a1fb00-0000-7000-8000-000000000003'::uuid, 'hv_acme_bulb1',   '01a1fa00-0000-7000-8000-000000000002'::uuid, 'Bedside Bulb', 'SEED-LIGHT-2',  true,  0),
    ('01a1fb00-0000-7000-8000-000000000004'::uuid, 'hv_acme_plug1',   '01a1fa00-0000-7000-8000-000000000003'::uuid, 'Coffee Plug',  'SEED-PLUG-1',   true,  0),
    ('01a1fb00-0000-7000-8000-000000000005'::uuid, 'hv_acme_bulb1',   '01a1fa00-0000-7000-8000-000000000003'::uuid, 'Pantry Bulb',  'SEED-LIGHT-3',  false, 240)
) AS v(id, product_key, room_id, name, serial, online, silent_for)
JOIN products p ON p.product_key = v.product_key
WHERE NOT EXISTS (SELECT 1 FROM devices d WHERE d.id = v.id);

-- Re-running the seed refreshes the demo state: the presence sweep will have marked anything that
-- never speaks as offline, which is correct for a real device and useless for a screenshot. Only
-- rows whose serial starts with SEED- are touched, so a real paired device is never rewritten.
UPDATE devices d
SET online = v.online,
    last_seen_at = now() - (v.silent_for || ' minutes')::interval
FROM (
  VALUES
    ('SEED-LIGHT-1',  true,  0),
    ('SEED-SENSOR-1', true,  0),
    ('SEED-LIGHT-2',  true,  0),
    ('SEED-PLUG-1',   true,  0),
    ('SEED-LIGHT-3',  false, 240)
) AS v(serial, online, silent_for)
WHERE d.serial = v.serial AND d.home_id = :demo_home;

-- Points, one row per (device, code). The values are what the demo should *show*, so the lights are
-- on, the plug is drawing a trickle and the sensor is reading a room.
INSERT INTO device_points (id, device_id, vendor_id, capability, slot, code, dp, kind, writable,
                           unit, min_val, max_val, value_num, value_bool, updated_at)
SELECT gen_random_uuid(), v.device_id, :vendor, v.capability, v.slot,
       v.capability || '.' || v.slot, v.dp, v.kind, v.writable, v.unit, v.min_val, v.max_val,
       v.value_num, v.value_bool, now() - (v.age || ' seconds')::interval
FROM (
  VALUES
    -- Floor lamp: on, warm, at 64 %
    ('01a1fb00-0000-7000-8000-000000000001'::uuid, 'switch',      'power',   '1', 'bool',  true,  null, null,  null,   null, true,  30),
    ('01a1fb00-0000-7000-8000-000000000001'::uuid, 'brightness',  'level',   '2', 'int',   true,  '%',  0,     100,   64,    null,  30),
    ('01a1fb00-0000-7000-8000-000000000001'::uuid, 'color_temp',  'kelvin',  '3', 'int',   true,  'K',  2000,  6500,  2700,  null,  30),
    -- Room sensor: 21.5 °C, 58 % RH, 87 % battery
    ('01a1fb00-0000-7000-8000-000000000002'::uuid, 'temperature', 'value',   '1', 'float', false, '°C', -10,   50,    21.5,  null,  30),
    ('01a1fb00-0000-7000-8000-000000000002'::uuid, 'humidity',    'value',   '2', 'float', false, '%',  0,     100,   58,    null,  30),
    ('01a1fb00-0000-7000-8000-000000000002'::uuid, 'battery',     'level',   '3', 'int',   false, '%',  0,     100,   87,    null,  30),
    -- Bedside bulb: on, low, very warm
    ('01a1fb00-0000-7000-8000-000000000003'::uuid, 'switch',      'power',   '1', 'bool',  true,  null, null,  null,  null, true,  30),
    ('01a1fb00-0000-7000-8000-000000000003'::uuid, 'brightness',  'level',   '2', 'int',   true,  '%',  0,     100,   20,    null,  30),
    ('01a1fb00-0000-7000-8000-000000000003'::uuid, 'color_temp',  'kelvin',  '3', 'int',   true,  'K',  2000,  6500,  2200,  null,  30),
    -- Coffee plug: on and drawing 4.2 W (standby)
    ('01a1fb00-0000-7000-8000-000000000004'::uuid, 'switch',      'power',   '1', 'bool',  true,  null, null,  null,  null, true,  30),
    ('01a1fb00-0000-7000-8000-000000000004'::uuid, 'energy',      'watts',   '2', 'float', false, 'W',  0,     2500,  4.2,   null,  30),
    -- Pantry bulb: offline for four hours, last known off
    ('01a1fb00-0000-7000-8000-000000000005'::uuid, 'switch',      'power',   '1', 'bool',  true,  null, null,  null,  null, false, 14400),
    ('01a1fb00-0000-7000-8000-000000000005'::uuid, 'brightness',  'level',   '2', 'int',   true,  '%',  0,     100,   0,     null,  14400)
) AS v(device_id, capability, slot, dp, kind, writable, unit, min_val, max_val, value_num, value_bool, age)
-- Upsert, not skip: the demo's *values* are the point of the demo, and a load run or a stray command
-- will have overwritten them. The unique index is (device_id, code).
ON CONFLICT (device_id, code) DO UPDATE
SET value_num = EXCLUDED.value_num,
    value_bool = EXCLUDED.value_bool,
    updated_at = now();

-- A day of room temperature, so the hero has a shape: cool at night, warm by mid-morning.
INSERT INTO telemetry (ts, device_id, point_id, vendor_id, num)
SELECT now() - (g || ' minutes')::interval,
       '01a1fb00-0000-7000-8000-000000000002'::uuid,
       p.id,
       :vendor,
       round((19.4
              + 1.6 * sin(((extract(hour FROM (now() - (g || ' minutes')::interval)) - 6) / 24.0) * 2 * pi())
              + 0.35 * sin(g / 7.0))::numeric, 2)
FROM generate_series(0, 1439, 15) AS g
JOIN device_points p
  ON p.device_id = '01a1fb00-0000-7000-8000-000000000002'::uuid
 AND p.code = 'temperature.value'
WHERE NOT EXISTS (
  SELECT 1 FROM telemetry t
  WHERE t.device_id = '01a1fb00-0000-7000-8000-000000000002'::uuid
    AND t.ts = now() - (g || ' minutes')::interval
);

COMMIT;
