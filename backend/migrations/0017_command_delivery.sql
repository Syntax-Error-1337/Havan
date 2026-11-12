-- Durable delivery for commands the broker never accepted.
--
-- A command refused during an outage is settled `unreachable`. Until now that was the end
-- of it: the customer's client was told to retry and a server-initiated command simply
-- never happened. `attempts` records how many times delivery was tried, so a redelivery
-- loop can be bounded and an operator can tell "one blip" from "a device that never comes
-- back".
ALTER TABLE commands ADD COLUMN IF NOT EXISTS attempts integer NOT NULL DEFAULT 0;

-- The redelivery sweep looks only at recently refused commands; older rows keep their
-- history and are never retried. A partial index keeps that scan cheap on a busy table.
CREATE INDEX IF NOT EXISTS commands_redelivery_idx
    ON commands (requested_at)
    WHERE status = 'unreachable';

COMMENT ON COLUMN commands.attempts IS
    'Delivery attempts made, including redeliveries after a broker outage.';
