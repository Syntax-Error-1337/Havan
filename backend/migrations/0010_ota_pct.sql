-- A device reporting how far through the download it is. The contract promises
-- `pct`, so it has somewhere to land instead of being dropped on the floor.
ALTER TABLE ota_jobs ADD COLUMN pct smallint NOT NULL DEFAULT 0;
ALTER TABLE ota_jobs ADD CONSTRAINT ota_jobs_pct_check CHECK (pct BETWEEN 0 AND 100);
