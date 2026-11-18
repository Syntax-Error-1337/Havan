-- Home invitations: adding somebody who has not signed up yet.
--
-- Until now `POST /homes/{id}/members` was not an invitation at all — it required the address to belong to an existing
-- account and added that person immediately, 404 otherwise. For a family home that is the wrong shape: the person you
-- want to add is usually the one who has never installed the app, and "ask them to create an account first" is a support
-- message, not a feature. The vendor side has had invitations since 0025; this is the same idea for a home.
--
-- The token is stored hashed, like a password and like a vendor invitation: an invitation row is a credential, and a
-- leaked backup must not be a set of working invitations. It is returned in plaintext exactly once, at creation, because
-- the platform still has no mail transport — the person who creates it sends it.
CREATE TABLE home_invites (
    id               uuid PRIMARY KEY,
    home_id          uuid NOT NULL REFERENCES homes(id) ON DELETE CASCADE,
    email            text NOT NULL,
    -- The owner row is the person who created the home; promoting somebody to it is a transfer, not an invitation.
    role             text NOT NULL CHECK (role IN ('admin', 'member', 'guest')),
    token_hash       text NOT NULL UNIQUE,
    invited_by       uuid NOT NULL,
    created_at       timestamptz NOT NULL DEFAULT now(),
    expires_at       timestamptz NOT NULL,
    accepted_at      timestamptz,
    accepted_user_id uuid
);

CREATE INDEX home_invites_home_idx ON home_invites (home_id, created_at DESC);

-- One live invitation per address per home. Inviting the same person twice replaces nothing and fails, so the admin
-- revokes or resends the one that exists — which is what keeps "why did I get three of these" from happening.
CREATE UNIQUE INDEX home_invites_pending_idx
    ON home_invites (home_id, lower(email))
    WHERE accepted_at IS NULL;

ALTER TABLE home_invites ENABLE ROW LEVEL SECURITY;
ALTER TABLE home_invites FORCE ROW LEVEL SECURITY;

-- Readable by the home's own members; the token is what accepts it, and accepting is the one path that does not need a
-- policy to see the row first (the service looks it up by hash with the system context).
CREATE POLICY home_invites_scope ON home_invites FOR ALL
    USING (
        haven_user() IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM home_members m
             WHERE m.home_id = home_invites.home_id AND m.user_id = haven_user()
        )
    );

COMMENT ON TABLE home_invites IS
    'Pending invitations to a home. Token stored as a SHA-256 hash; plaintext returned once.';
