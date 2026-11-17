-- How often a device is *supposed* to report, copied from its product at claim.
--
-- The app decides "this device has gone quiet" from a threshold. One threshold for every device on the
-- platform is wrong in both directions: an hourly sensor would look silent almost always, and a plug
-- that reports every ten seconds would hide a dropout for a quarter of an hour. The number belongs to
-- the product — the vendor knows how their hardware behaves — and is copied onto the device at claim in
-- the same way `category` and `product_key` already are, because a customer request carries no vendor
-- and `products` is vendor-scoped (RLS).
--
-- NULL means "not declared", and readers fall back to the platform default rather than guessing.

ALTER TABLE devices
    ADD COLUMN IF NOT EXISTS report_interval_secs integer;

COMMENT ON COLUMN devices.report_interval_secs IS
    'Seconds between expected reports, copied from the product''s thing model at claim. NULL = not declared.';

-- Backfill what the catalogue already knows. Guarded on the key existing, so a product that never
-- declared one stays NULL instead of failing the migration on a bad cast.
UPDATE devices d
SET report_interval_secs = (p.thing_model ->> 'report_interval_secs')::int
FROM products p
WHERE p.id = d.product_id
  AND d.report_interval_secs IS NULL
  AND p.thing_model ? 'report_interval_secs'
  AND (p.thing_model ->> 'report_interval_secs') ~ '^[0-9]+$'
  AND (p.thing_model ->> 'report_interval_secs')::int BETWEEN 1 AND 86400;
