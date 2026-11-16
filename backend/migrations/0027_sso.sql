-- SSO: the data model a vendor needs before an identity provider can be wired up.
--
-- Deliberately no enforcement yet. `sso_required` records the vendor's decision, and `sso_subject`
-- records which IdP identity belongs to which account. Flipping password login off before an
-- assertion can be validated would lock a vendor out of its own portal with no way back in — so the
-- flag is stored, reported, and checked by nothing until the provider is real.
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS sso_required boolean NOT NULL DEFAULT false;
-- The email domain that identifies this vendor's people. Used to refuse binding an identity from a
-- domain the vendor does not own; the assertion itself carries the authoritative claim.
ALTER TABLE vendors ADD COLUMN IF NOT EXISTS sso_domain text;

ALTER TABLE vendor_users ADD COLUMN IF NOT EXISTS sso_subject text;

-- One IdP identity belongs to one account. Two rows claiming the same subject is a provisioning
-- mistake, and it is better caught here than at the next login.
CREATE UNIQUE INDEX IF NOT EXISTS vendor_users_sso_subject_idx
    ON vendor_users (vendor_id, sso_subject)
    WHERE sso_subject IS NOT NULL;

COMMENT ON COLUMN vendors.sso_required IS
    'The vendor has asked to require SSO. Enforcement lands with the IdP integration; see ADR-005.';
COMMENT ON COLUMN vendor_users.sso_subject IS
    'The `sub` claim from the vendor''s identity provider. Bound explicitly, never inferred from email.';
