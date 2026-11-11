-- 0003 — devices, their points, telemetry, commands, pairing, OTA.
--
-- `vendor_id` is denormalised onto points and telemetry on purpose: it makes the
-- RLS policy a column comparison instead of a join, and a join-based policy on a
-- large table is a query plan nobody wants.

CREATE TABLE devices (
    id           uuid PRIMARY KEY,
    vendor_id    uuid NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    product_id   uuid NOT NULL REFERENCES products(id),
    home_id      uuid REFERENCES homes(id) ON DELETE SET NULL,
    room_id      uuid REFERENCES rooms(id) ON DELETE SET NULL,
    name         text NOT NULL,
    serial       text NOT NULL,
    cert_serial  text,
    secret_hash  text NOT NULL,
    fw_version   text,
    online       boolean NOT NULL DEFAULT false,
    last_seen_at timestamptz,
    transport    text NOT NULL DEFAULT 'cloud' CHECK (transport IN ('cloud','lan')),
    ip           inet,
    state        text NOT NULL DEFAULT 'unclaimed'
                 CHECK (state IN ('unclaimed','joining','claimed','released')),
    claimed_at   timestamptz,
    metadata     jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at   timestamptz NOT NULL DEFAULT now(),
    UNIQUE (product_id, serial)
);

CREATE INDEX devices_home_idx ON devices (home_id, created_at DESC);
CREATE INDEX devices_vendor_idx ON devices (vendor_id, created_at DESC);
CREATE INDEX devices_online_idx ON devices (vendor_id, online) WHERE online;

CREATE TABLE device_points (
    id          uuid PRIMARY KEY,
    device_id   uuid NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    vendor_id   uuid NOT NULL,
    capability  text NOT NULL,
    slot        text NOT NULL,
    code        text NOT NULL,
    dp          text NOT NULL,
    kind        text NOT NULL,
    writable    boolean NOT NULL DEFAULT false,
    unit        text,
    min_val     double precision,
    max_val     double precision,
    scale       double precision,
    enum_map    jsonb,
    value_num   double precision,
    value_bool  boolean,
    value_text  text,
    updated_at  timestamptz,
    UNIQUE (device_id, code)
);

CREATE INDEX device_points_device_idx ON device_points (device_id);
CREATE INDEX device_points_dp_idx ON device_points (device_id, dp);

-- Telemetry is a plain range-partitioned table: monthly partitions, BRIN on the
-- time column, and retention that drops a whole partition. No Timescale licence
-- in the hosting path (ADR-002); hourly rollups live in `telemetry_rollup`.
CREATE TABLE telemetry (
    ts        timestamptz NOT NULL,
    device_id uuid NOT NULL,
    point_id  uuid NOT NULL,
    vendor_id uuid NOT NULL,
    num       double precision,
    bool      boolean,
    txt       text
) PARTITION BY RANGE (ts);

-- 24 months up front; the retention job keeps the window rolling and drops the
-- oldest partition, which is a metadata operation rather than a mass delete.
DO $$
DECLARE
    start_month date := date_trunc('month', now())::date;
    i integer;
    part_name text;
BEGIN
    FOR i IN 0..23 LOOP
        part_name := format('telemetry_%s', to_char(start_month + (i || ' month')::interval, 'YYYY_MM'));
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS %I PARTITION OF telemetry FOR VALUES FROM (%L) TO (%L)',
            part_name,
            start_month + (i || ' month')::interval,
            start_month + ((i + 1) || ' month')::interval
        );
    END LOOP;
END
$$;

-- A device with a wrong clock still lands somewhere instead of failing its batch.
CREATE TABLE telemetry_overflow PARTITION OF telemetry DEFAULT;

CREATE INDEX telemetry_ts_brin ON telemetry USING brin (ts) WITH (pages_per_range = 32);
CREATE INDEX telemetry_point_idx ON telemetry (point_id, ts DESC);
CREATE INDEX telemetry_device_idx ON telemetry (device_id, ts DESC);

-- Hourly aggregates, maintained by the worker. Timescale would have done this
-- with a continuous aggregate; owning it is the price of the licence.
CREATE TABLE telemetry_rollup (
    point_id  uuid NOT NULL,
    bucket    timestamptz NOT NULL,
    vendor_id uuid NOT NULL,
    min_num   double precision,
    max_num   double precision,
    avg_num   double precision,
    sum_num   double precision,
    samples   integer NOT NULL DEFAULT 0,
    PRIMARY KEY (point_id, bucket)
);

CREATE TABLE commands (
    id              uuid PRIMARY KEY,
    vendor_id       uuid NOT NULL,
    device_id       uuid NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    point_id        uuid REFERENCES device_points(id) ON DELETE SET NULL,
    op              text NOT NULL CHECK (op IN ('set','get','action')),
    payload         jsonb NOT NULL DEFAULT '{}'::jsonb,
    desired         jsonb,
    reported        jsonb,
    status          text NOT NULL DEFAULT 'queued'
                    CHECK (status IN ('queued','dispatched','applied','rejected','timeout','unreachable')),
    idempotency_key text,
    requested_by    uuid,
    requested_at    timestamptz NOT NULL DEFAULT now(),
    dispatched_at   timestamptz,
    acked_at        timestamptz,
    error_code      text,
    dispatch_path   text,
    UNIQUE (device_id, idempotency_key)
);

CREATE INDEX commands_pending_idx ON commands (status, requested_at)
    WHERE status IN ('queued','dispatched');
CREATE INDEX commands_device_idx ON commands (device_id, requested_at DESC);

CREATE TABLE pairing_tickets (
    id            uuid PRIMARY KEY,
    home_id       uuid NOT NULL REFERENCES homes(id) ON DELETE CASCADE,
    vendor_id     uuid NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    user_id       uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    jti           uuid NOT NULL UNIQUE,
    nonce_hash    text NOT NULL,
    expires_at    timestamptz NOT NULL,
    consumed_at   timestamptz,
    device_id     uuid,
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX pairing_tickets_home_idx ON pairing_tickets (home_id, created_at DESC);

CREATE TABLE provisioning_sessions (
    id            uuid PRIMARY KEY,
    ticket_id     uuid REFERENCES pairing_tickets(id) ON DELETE SET NULL,
    home_id       uuid NOT NULL,
    serial        text,
    transport     text NOT NULL CHECK (transport IN ('ble','ap','qr','matter')),
    stage         text NOT NULL DEFAULT 'started',
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE ota_campaigns (
    id           uuid PRIMARY KEY,
    vendor_id    uuid NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    product_id   uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    firmware_id  uuid NOT NULL REFERENCES firmware(id),
    rollout_pct  integer NOT NULL DEFAULT 100 CHECK (rollout_pct BETWEEN 0 AND 100),
    status       text NOT NULL DEFAULT 'draft'
                 CHECK (status IN ('draft','running','paused','halted','complete')),
    created_at   timestamptz NOT NULL DEFAULT now(),
    started_at   timestamptz,
    halted_at    timestamptz
);

CREATE TABLE ota_jobs (
    campaign_id uuid NOT NULL REFERENCES ota_campaigns(id) ON DELETE CASCADE,
    device_id   uuid NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    state       text NOT NULL DEFAULT 'pending'
                CHECK (state IN ('pending','sent','downloading','verified','rebooting','success','failed','skipped')),
    attempts    integer NOT NULL DEFAULT 0,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (campaign_id, device_id)
);

CREATE INDEX ota_jobs_state_idx ON ota_jobs (campaign_id, state);

CREATE TABLE mqtt_acl (
    vendor_id uuid NOT NULL,
    device_id uuid NOT NULL,
    home_id   uuid,
    UNIQUE (device_id)
);

CREATE TABLE audit_log (
    id        uuid PRIMARY KEY,
    vendor_id uuid,
    actor     text NOT NULL,
    action    text NOT NULL,
    subject   text,
    ip        inet,
    meta      jsonb NOT NULL DEFAULT '{}'::jsonb,
    ts        timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX audit_log_vendor_idx ON audit_log (vendor_id, ts DESC);

-- ---------------------------------------------------------------------------
-- Row-level security. FORCE makes the table owner subject too, so a developer
-- connecting as the owner in production still cannot read across tenants.
-- ---------------------------------------------------------------------------

ALTER TABLE vendors         ENABLE ROW LEVEL SECURITY;
ALTER TABLE vendor_users    ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys        ENABLE ROW LEVEL SECURITY;
ALTER TABLE products        ENABLE ROW LEVEL SECURITY;
ALTER TABLE firmware        ENABLE ROW LEVEL SECURITY;
ALTER TABLE devices         ENABLE ROW LEVEL SECURITY;
ALTER TABLE device_points   ENABLE ROW LEVEL SECURITY;
ALTER TABLE commands        ENABLE ROW LEVEL SECURITY;
ALTER TABLE ota_campaigns   ENABLE ROW LEVEL SECURITY;
ALTER TABLE ota_jobs        ENABLE ROW LEVEL SECURITY;
ALTER TABLE mqtt_acl        ENABLE ROW LEVEL SECURITY;
ALTER TABLE telemetry       ENABLE ROW LEVEL SECURITY;
ALTER TABLE telemetry_rollup ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_log       ENABLE ROW LEVEL SECURITY;

ALTER TABLE vendors         FORCE ROW LEVEL SECURITY;
ALTER TABLE vendor_users    FORCE ROW LEVEL SECURITY;
ALTER TABLE api_keys        FORCE ROW LEVEL SECURITY;
ALTER TABLE products        FORCE ROW LEVEL SECURITY;
ALTER TABLE firmware        FORCE ROW LEVEL SECURITY;
ALTER TABLE devices         FORCE ROW LEVEL SECURITY;
ALTER TABLE device_points   FORCE ROW LEVEL SECURITY;
ALTER TABLE commands        FORCE ROW LEVEL SECURITY;
ALTER TABLE ota_campaigns   FORCE ROW LEVEL SECURITY;
ALTER TABLE ota_jobs        FORCE ROW LEVEL SECURITY;
ALTER TABLE mqtt_acl        FORCE ROW LEVEL SECURITY;
ALTER TABLE telemetry       FORCE ROW LEVEL SECURITY;
ALTER TABLE telemetry_rollup FORCE ROW LEVEL SECURITY;
ALTER TABLE audit_log       FORCE ROW LEVEL SECURITY;

-- A helper that returns NULL (never an error) when the setting is absent, so an
-- unscoped connection sees zero rows instead of a cast failure.
CREATE OR REPLACE FUNCTION haven_vendor() RETURNS uuid
LANGUAGE sql STABLE AS $$
    SELECT NULLIF(current_setting('haven.vendor_id', true), '')::uuid
$$;

CREATE OR REPLACE FUNCTION haven_user() RETURNS uuid
LANGUAGE sql STABLE AS $$
    SELECT NULLIF(current_setting('haven.user_id', true), '')::uuid
$$;

CREATE POLICY vendor_scope ON vendors FOR ALL
    USING (id = haven_vendor());

CREATE POLICY vendor_scope ON vendor_users FOR ALL
    USING (vendor_id = haven_vendor());

CREATE POLICY vendor_scope ON api_keys FOR ALL
    USING (vendor_id = haven_vendor());

CREATE POLICY vendor_scope ON products FOR ALL
    USING (vendor_id = haven_vendor());

CREATE POLICY vendor_scope ON firmware FOR ALL
    USING (product_id IN (SELECT id FROM products WHERE vendor_id = haven_vendor()));

-- Devices are reachable two ways: by the vendor that made them, and by a member
-- of the home they belong to. Policies are OR'd, which is exactly the rule.
CREATE POLICY vendor_scope ON devices FOR ALL
    USING (vendor_id = haven_vendor());

CREATE POLICY home_member_scope ON devices FOR ALL
    USING (
        home_id IS NOT NULL
        AND haven_user() IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM home_members m
            WHERE m.home_id = devices.home_id AND m.user_id = haven_user()
        )
    );

CREATE POLICY vendor_scope ON device_points FOR ALL
    USING (vendor_id = haven_vendor());

CREATE POLICY home_member_scope ON device_points FOR ALL
    USING (
        haven_user() IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM devices d
            JOIN home_members m ON m.home_id = d.home_id
            WHERE d.id = device_points.device_id AND m.user_id = haven_user()
        )
    );

CREATE POLICY vendor_scope ON commands FOR ALL
    USING (vendor_id = haven_vendor());

CREATE POLICY home_member_scope ON commands FOR ALL
    USING (
        haven_user() IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM devices d
            JOIN home_members m ON m.home_id = d.home_id
            WHERE d.id = commands.device_id AND m.user_id = haven_user()
        )
    );

CREATE POLICY vendor_scope ON ota_campaigns FOR ALL
    USING (vendor_id = haven_vendor());

CREATE POLICY vendor_scope ON ota_jobs FOR ALL
    USING (device_id IN (SELECT id FROM devices WHERE vendor_id = haven_vendor()));

CREATE POLICY vendor_scope ON mqtt_acl FOR ALL
    USING (vendor_id = haven_vendor());

CREATE POLICY vendor_scope ON telemetry FOR ALL
    USING (vendor_id = haven_vendor());

CREATE POLICY vendor_scope ON telemetry_rollup FOR ALL
    USING (vendor_id = haven_vendor());

CREATE POLICY home_member_scope ON telemetry FOR ALL
    USING (
        haven_user() IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM devices d
            JOIN home_members m ON m.home_id = d.home_id
            WHERE d.id = telemetry.device_id AND m.user_id = haven_user()
        )
    );

CREATE POLICY vendor_scope ON audit_log FOR ALL
    USING (vendor_id = haven_vendor());

GRANT haven_system TO CURRENT_USER;
