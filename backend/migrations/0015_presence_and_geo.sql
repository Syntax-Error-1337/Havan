-- 0015 — where the home is, and who is in it.
--
-- Two facts the automation phase needs and nothing else did:
--
--   * `geo_lat`/`geo_lon` already exist on homes but nothing ever wrote them, so
--     "at sunset" had nothing to compute from.
--   * `present` is per member, not per home: presence is a claim each person makes
--     about themselves, and a single home-wide flag would be a lie the moment two
--     people share a flat.

ALTER TABLE home_members
    ADD COLUMN IF NOT EXISTS present boolean NOT NULL DEFAULT true;

-- Backfill is not needed: `true` is the honest default for someone who joined a home
-- and has never said otherwise.
COMMENT ON COLUMN home_members.present IS
    'Whether this member says they are at home. Aggregated to "anyone home" for automation conditions.';

COMMENT ON COLUMN homes.geo_lat IS
    'Latitude for sunrise/sunset. NULL means solar automation refuses to be created.';
COMMENT ON COLUMN homes.geo_lon IS
    'Longitude for sunrise/sunset.';
