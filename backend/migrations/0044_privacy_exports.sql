-- A receipt for every data export.
--
-- `GET /me/export` streamed a bundle and left no trace: the screen that offers "a copy of your data" had no way to say
-- whether one had ever been produced, and a subject-access request (Article 15, and the consumer version of it) is
-- answered by a record as much as by a file. This is the record — who asked, when, and how large the answer was.
--
-- Deliberately not the bundle itself: storing what was exported would mean keeping a second copy of everything the
-- person asked us to hand them, which is the opposite of a privacy feature.
CREATE TABLE privacy_exports (
    id         uuid PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    bytes      bigint NOT NULL DEFAULT 0
);

CREATE INDEX privacy_exports_user_idx ON privacy_exports (user_id, created_at DESC);

ALTER TABLE privacy_exports ENABLE ROW LEVEL SECURITY;
ALTER TABLE privacy_exports FORCE ROW LEVEL SECURITY;

-- A person sees their own receipts. The service writes them as the platform, which is the only writer there is.
CREATE POLICY privacy_exports_self ON privacy_exports FOR ALL
    USING (user_id = haven_user());

COMMENT ON TABLE privacy_exports IS
    'One row per data export. A receipt, not a copy: the bundle itself is never stored.';
