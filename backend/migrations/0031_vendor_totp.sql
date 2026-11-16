-- Two-factor authentication for the people who can publish firmware into strangers' homes.

ALTER TABLE vendors
    ADD COLUMN totp_required boolean NOT NULL DEFAULT false;

COMMENT ON COLUMN vendors.totp_required IS
    'A vendor policy: every member must present a TOTP code at sign-in. Enforced only for members who have enrolled, and the toggle refuses to turn on while somebody has not.';

ALTER TABLE vendor_users
    ADD COLUMN totp_confirmed_at timestamptz;

COMMENT ON COLUMN vendor_users.totp_confirmed_at IS
    'When the member proved they can generate codes. NULL with a secret set means enrolment started but was never confirmed — a code cannot be trusted yet.';
