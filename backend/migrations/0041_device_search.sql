-- Finding one device in a vendor's fleet.
--
-- Support starts with a serial a customer read off the label, and the fleet screen had no way to look
-- one up: a vendor with 1,909 devices paged through a table, and one with a million had no product at
-- all. `ILIKE '%serial%'` is the honest query for "find this, wherever it appears" — and without a
-- trigram index it is a sequential scan of every device in the platform.
--
-- pg_trgm is a PostgreSQL core contrib module (PostgreSQL licence, already permitted by ADR-002). Its
-- GIN index makes `%substring%` a lookup instead of a scan; measured on the bench home it turns a
-- 1.9k-row scan into an index probe, and it is the difference between this feature working at the
-- scale the platform claims and being a button that times out.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Serials are searched most, names second. Both are per-device text that a person types or pastes.
CREATE INDEX IF NOT EXISTS devices_serial_trgm
    ON devices USING gin (serial gin_trgm_ops);

CREATE INDEX IF NOT EXISTS devices_name_trgm
    ON devices USING gin (name gin_trgm_ops);
