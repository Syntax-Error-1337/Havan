-- Why a job gave up. Without it, a fleet that failed at 3 a.m. is just a row of
-- 'failed' with no way to tell a bad image from a device that never answered.
ALTER TABLE ota_jobs ADD COLUMN error text;

-- The sweep scans by state and age; the existing index is (campaign_id, state),
-- which cannot answer "every sent job older than an hour" across campaigns.
CREATE INDEX ota_jobs_state_updated_idx ON ota_jobs (state, updated_at);
