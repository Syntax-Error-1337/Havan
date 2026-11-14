-- Push delivery.
--
-- `notifications` has been accumulating rows since the automation engine landed, and
-- `push_tokens` has been an empty table: nothing ever sent anything, so an alert reached the
-- app only while its websocket happened to be open. This is the column a sender needs.
--
-- `delivered_at` is set whether or not a device token existed: a notification with nobody to
-- deliver it to is *done*, not pending, and a queue that keeps retrying it forever is a queue
-- that grows without bound.
ALTER TABLE notifications ADD COLUMN IF NOT EXISTS delivered_at timestamp with time zone;
ALTER TABLE notifications ADD COLUMN IF NOT EXISTS delivery_attempts integer NOT NULL DEFAULT 0;
ALTER TABLE notifications ADD COLUMN IF NOT EXISTS delivery_error text;

-- The sender asks "what has not been delivered", so the index is partial and ordered by age.
CREATE INDEX IF NOT EXISTS notifications_undelivered_idx
    ON notifications (created_at)
    WHERE delivered_at IS NULL;

COMMENT ON COLUMN notifications.delivery_attempts IS
    'Push attempts. A notification is abandoned after a few, not retried forever.';
