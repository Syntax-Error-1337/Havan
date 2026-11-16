-- Vendor teams: an invitation that expires, and a role that means something.
--
-- A vendor is a company, not an account. The first person in is the owner, and the way a second
-- person arrives is an invitation — not a shared password, which is what "we all log in as the
-- admin" degrades into.
--
-- The token is stored hashed, like a password: an invitation in the database is a credential, and
-- a leaked backup should not be a set of working invitations.
CREATE TABLE vendor_invites (
    id               uuid PRIMARY KEY,
    vendor_id        uuid NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    email            text NOT NULL,
    -- Owner is not invitable: it is the role the first account has, and promoting someone to it is
    -- an explicit transfer, not an invitation.
    role             text NOT NULL CHECK (role IN ('admin', 'developer', 'viewer')),
    token_hash       text NOT NULL UNIQUE,
    invited_by       uuid NOT NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    expires_at       timestamptz NOT NULL,
    accepted_at      timestamptz,
    accepted_user_id uuid
);

CREATE INDEX vendor_invites_vendor_idx ON vendor_invites (vendor_id, created_at DESC);

-- One live invitation per address per vendor. Re-inviting the same person replaces nothing; it
-- fails, and the operator revokes or resends the one that exists, which is the behaviour that
-- keeps "why do I have three invitation emails" from happening.
CREATE UNIQUE INDEX vendor_invites_pending_idx
    ON vendor_invites (vendor_id, lower(email))
    WHERE accepted_at IS NULL;

ALTER TABLE vendor_invites ENABLE ROW LEVEL SECURITY;
ALTER TABLE vendor_invites FORCE ROW LEVEL SECURITY;

CREATE POLICY vendor_scope ON vendor_invites FOR ALL
    USING (vendor_id = haven_vendor());

COMMENT ON TABLE vendor_invites IS
    'Pending team invitations. Token stored as a SHA-256 hash; plaintext returned once.';
