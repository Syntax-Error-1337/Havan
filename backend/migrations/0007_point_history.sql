-- 0007 — the storage policy travels with the point.
--
-- `history` comes from the capability catalog when the device is claimed. Ingest
-- reads it to decide whether a report also becomes a telemetry row, which is
-- what keeps a chatty power sensor from dominating the hypertable.

ALTER TABLE device_points ADD COLUMN history text NOT NULL DEFAULT 'change';

-- Existing rows: sensors keep history, switches keep changes, diagnostics and
-- light settings do not. A one-off backfill of a dev database only.
UPDATE device_points SET history = 'off'
 WHERE capability IN ('brightness', 'color_temp', 'color_rgb', 'rssi', 'media.volume');
