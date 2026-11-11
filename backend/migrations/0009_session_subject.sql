-- 0009 — a session's subject is a customer or a vendor user, not both.
--
-- `refresh_tokens.user_id` referenced `users`, so issuing a token for a portal
-- login failed the FK. The subject is now one of two columns with a CHECK, which
-- keeps referential integrity on both sides instead of dropping the FK.

ALTER TABLE refresh_tokens ALTER COLUMN user_id DROP NOT NULL;
ALTER TABLE refresh_tokens ADD COLUMN vendor_user_id uuid
    REFERENCES vendor_users(id) ON DELETE CASCADE;

ALTER TABLE refresh_tokens ADD CONSTRAINT refresh_tokens_one_subject CHECK (
    (user_id IS NOT NULL AND vendor_user_id IS NULL)
    OR (user_id IS NULL AND vendor_user_id IS NOT NULL)
);

CREATE INDEX refresh_tokens_vendor_user_idx ON refresh_tokens (vendor_user_id)
    WHERE vendor_user_id IS NOT NULL;
