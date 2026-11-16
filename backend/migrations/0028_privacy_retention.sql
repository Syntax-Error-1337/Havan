-- The window between "erased" and "gone".
--
-- `DELETE /me` removes a person's memberships, homes, devices, alerts and sessions and turns the
-- account into a tombstone. The tombstone itself, and the receipts in `data_requests`, are the part
-- a regulator expects us to be able to explain, so they outlive the data by a period the vendor's
-- own licence sets.
--
-- The window lives on the grant because retention is a term of the contract the vendor bought, not
-- a platform-wide constant: an EU reseller and a US one do not buy the same agreement, and the
-- licence is the row where their terms already live.

ALTER TABLE licence_grants
    ADD COLUMN retention_days integer NOT NULL DEFAULT 90,
    ADD CONSTRAINT licence_grants_retention_days_check
        CHECK (retention_days BETWEEN 1 AND 3650);

COMMENT ON COLUMN licence_grants.retention_days IS
    'Days after erasure that the tombstone and the receipts are kept, from the licence agreement.';

-- When the tombstone goes. Set at erasure time, not recomputed later: a grant that expires or is
-- renegotiated afterwards must not move the finish line for a person who has already left, in
-- either direction — extending it would keep data we promised to drop, and shortening it would
-- destroy a receipt a complaint might still need.
ALTER TABLE users ADD COLUMN purge_after timestamptz;

CREATE INDEX users_purge_after_idx ON users (purge_after) WHERE purge_after IS NOT NULL;

COMMENT ON COLUMN users.purge_after IS
    'When this erased account may be deleted outright. NULL means not erased, or not yet scheduled.';
