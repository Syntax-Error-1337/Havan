-- Device certificates, and the date they stop working.
--
-- `cert_serial` already existed and was always null: nothing issued certificates. This adds
-- the expiry, because a certificate nobody tracks is a device that stops talking at 3 a.m.
-- on a random Tuesday and looks like a hardware fault.
ALTER TABLE devices ADD COLUMN IF NOT EXISTS cert_not_after timestamp with time zone;

-- The sweep reads "expiring soon", so the index is partial and ordered by the date it
-- compares: a fleet with ten thousand devices should not scan ten thousand rows a day.
CREATE INDEX IF NOT EXISTS devices_cert_expiry_idx
    ON devices (cert_not_after)
    WHERE cert_not_after IS NOT NULL;

COMMENT ON COLUMN devices.cert_not_after IS
    'Expiry of the device client certificate. NULL means the device has none (cloud-only, '
    'or enrolled before certificates existed).';
