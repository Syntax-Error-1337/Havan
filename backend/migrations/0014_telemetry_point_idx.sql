-- The throttle behind `history: interval:N` needs to answer "was this point
-- written recently?" on every report, so it needs an index that can answer it
-- without scanning the chunk. The only index on telemetry was a BRIN on ts,
-- which is right for range scans across a day and useless for one point.
CREATE INDEX IF NOT EXISTS telemetry_device_point_ts_idx
    ON telemetry (device_id, point_id, ts DESC);
