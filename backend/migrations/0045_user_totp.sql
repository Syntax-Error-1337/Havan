-- A second factor for a customer's own account.
--
-- The portal has had TOTP since the vendor work; the person who actually owns the devices in the house had nothing but a
-- password. This is the seed, one row per person who has turned it on.
--
-- `sealed_secret` is AES-256-GCM under HAVEN_VAULT_KEY, not a hash: verifying a code means recomputing it, so the server
-- has to be able to read the seed back. That makes this column equivalent to a password — a dump plus the key is a way
-- to mint valid codes — so the key lives in the environment and never in a table.
--
-- `last_step` is replay protection. A code is valid for its 30-second step plus one either side; without recording the
-- last step used, the same six digits could be replayed for a minute and a half after being shoulder-surfed.
CREATE TABLE user_totp (
    user_id        uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    sealed_secret  text NOT NULL,
    -- NULL while an enrolment is open: the secret exists but the person has not yet proved their authenticator works,
    -- and a second factor that is not proven is a way to lock yourself out.
    confirmed_at   timestamptz,
    last_step      bigint,
    created_at     timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE user_totp ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_totp FORCE ROW LEVEL SECURITY;

CREATE POLICY user_totp_self ON user_totp FOR ALL
    USING (user_id = haven_user());

COMMENT ON TABLE user_totp IS
    'One TOTP enrolment per account. The secret is sealed with HAVEN_VAULT_KEY; last_step stops a code being replayed.';
