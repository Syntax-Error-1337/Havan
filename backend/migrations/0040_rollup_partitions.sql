-- Partition the hourly rollups by month, the way raw readings already are.
--
-- `telemetry` was partitioned from the start and got a maintenance job. `telemetry_rollup` did not: one row per
-- point per hour is three orders of magnitude smaller than the readings it summarises, which is true right up
-- to the point where it is 8 760 rows per point per year. A million devices with five points each is 43
-- billion rows, and a retention job that deletes those in batches of forty thousand is a job that never
-- finishes. Dropping a month is one `DROP TABLE` — the same argument 0003 made for readings.
--
-- Written as a guarded `DO` block for two reasons:
--
--   * `haven-migrate` already runs each migration inside a transaction, so an explicit BEGIN/COMMIT here
--     committed the work early and left sqlx's own COMMIT reporting "there is no transaction in progress";
--   * re-running it must be harmless. A deploy that applied the conversion and then failed before the
--     bookkeeping row was written would otherwise fail on its next boot with "relation
--     telemetry_rollup_legacy already exists", during the one window where nobody wants to read a migration.

DO $$
DECLARE
    i integer;
    first_month timestamptz := date_trunc('month', now()) - interval '12 months';
BEGIN
    IF EXISTS (
        SELECT 1
          FROM pg_partitioned_table pt
          JOIN pg_class c ON c.oid = pt.partrelid
         WHERE c.relname = 'telemetry_rollup'
    ) THEN
        RAISE NOTICE 'telemetry_rollup is already partitioned; nothing to do';
        RETURN;
    END IF;

    ALTER TABLE telemetry_rollup RENAME TO telemetry_rollup_legacy;

    CREATE TABLE telemetry_rollup (
        point_id  uuid NOT NULL,
        bucket    timestamptz NOT NULL,
        vendor_id uuid NOT NULL,
        min_num   double precision,
        max_num   double precision,
        avg_num   double precision,
        sum_num   double precision,
        samples   integer NOT NULL DEFAULT 0,
        -- A partitioned table's primary key has to contain the partition key. `bucket` does, so the worker's
        -- `ON CONFLICT (point_id, bucket)` upsert keeps working unchanged.
        PRIMARY KEY (point_id, bucket)
    ) PARTITION BY RANGE (bucket);

    -- Twelve months of history plus a quarter ahead: the same window shape as `telemetry`, so an operator who
    -- has learned one table has learned both.
    FOR i IN 0..14 LOOP
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS %I PARTITION OF telemetry_rollup FOR VALUES FROM (%L) TO (%L)',
            format('telemetry_rollup_%s', to_char(first_month + make_interval(months => i), 'YYYY_MM')),
            first_month + make_interval(months => i),
            first_month + make_interval(months => i + 1)
        );
    END LOOP;

    -- A bucket computed from a device clock that disagrees with the calendar still lands somewhere.
    CREATE TABLE telemetry_rollup_overflow PARTITION OF telemetry_rollup DEFAULT;

    INSERT INTO telemetry_rollup (point_id, bucket, vendor_id, min_num, max_num, avg_num, sum_num, samples)
    SELECT point_id, bucket, vendor_id, min_num, max_num, avg_num, sum_num, samples
      FROM telemetry_rollup_legacy;

    DROP TABLE telemetry_rollup_legacy;

    -- The policies and the forced row security of 0003, re-applied to the table that now carries the name. A
    -- DROP/CREATE cycle silently loses both, and losing FORCE means the schema owner quietly sees every
    -- tenant's data.
    ALTER TABLE telemetry_rollup ENABLE ROW LEVEL SECURITY;
    ALTER TABLE telemetry_rollup FORCE ROW LEVEL SECURITY;

    CREATE POLICY vendor_scope ON telemetry_rollup FOR ALL
        USING (vendor_id = haven_vendor());

    GRANT SELECT, INSERT, UPDATE, DELETE ON telemetry_rollup TO haven_app, haven_system;

    COMMENT ON TABLE telemetry_rollup IS
        'Hourly aggregates, partitioned by month so retention is DROP TABLE rather than mass DELETE.';
END
$$;
