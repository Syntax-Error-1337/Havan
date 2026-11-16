-- Data subject requests: export and erasure.
--
-- A record that outlives the account it describes. That is why there is no foreign key to
-- `users`: the question "when was this person's data erased, and what did we remove" has to be
-- answerable after the account is gone, which is exactly when it is asked.
--
-- `detail` is the receipt: counts of what was deleted, not a copy of what was deleted.
CREATE TABLE data_requests (
    id           uuid PRIMARY KEY,
    user_id      uuid NOT NULL,
    kind         text NOT NULL CHECK (kind IN ('export', 'erase')),
    requested_at timestamptz NOT NULL DEFAULT now(),
    completed_at timestamptz,
    detail       jsonb NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX data_requests_user_idx ON data_requests (user_id, requested_at DESC);

COMMENT ON TABLE data_requests IS
    'Append-only receipt of export/erasure requests. No FK: kept after the account is erased.';

-- Erasure anonymises rather than deletes the `users` row, because `homes.owner_id` and
-- `device_points` history hang off it. The trailing `@invalid` is a reserved-looking domain that
-- cannot receive mail, so a half-completed erasure cannot silently deliver post to a stranger.
COMMENT ON COLUMN users.disabled_at IS
    'Set on erasure as well as on administrative disable: the row is a tombstone, not a person.';
