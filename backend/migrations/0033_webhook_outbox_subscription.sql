-- A delivery row belongs to one subscription, not to "whatever subscriptions match right now".
--
-- The outbox already wrote one row per subscription at enqueue time, but `due()` re-joined every
-- row to every active subscription with a matching event. With five endpoints that produced five
-- deliveries per row carrying the same outbox id, and `mark_delivered(id)` fired on the first
-- success — so four endpoints silently never received the event, and which one did was down to
-- join order. The row now names the subscription it was written for, and delivery reads that.
--
-- Nullable because rows queued before this migration have no way to say which endpoint they were
-- for; they are historical and expire with the outbox's retention.

-- Written idempotently on purpose: a column added by hand during an incident must not stop the same
-- migration from recording itself afterwards, and sqlx applies the file to a database whose state it
-- cannot know.
ALTER TABLE webhook_outbox
    ADD COLUMN IF NOT EXISTS subscription_id uuid REFERENCES vendor_webhooks (id) ON DELETE CASCADE;

CREATE INDEX IF NOT EXISTS webhook_outbox_subscription_idx
    ON webhook_outbox (subscription_id)
    WHERE delivered_at IS NULL;

-- Rows queued by an API instance that has not been restarted yet land without a subscription. During
-- a rolling deploy that window is real, so recover the unambiguous ones: a vendor with exactly one
-- endpoint subscribed to the event can only have meant that endpoint. Ambiguous rows are left NULL
-- rather than guessed at — a delivery to the wrong customer's URL is worse than a missed one.
UPDATE webhook_outbox o
   SET subscription_id = w.id
  FROM vendor_webhooks w
 WHERE o.subscription_id IS NULL
   AND w.vendor_id = o.vendor_id
   AND w.active
   AND o.event = ANY (w.events)
   AND (
        SELECT count(*)
          FROM vendor_webhooks w2
         WHERE w2.vendor_id = o.vendor_id
           AND w2.active
           AND o.event = ANY (w2.events)
       ) = 1;
