-- A push row is claimed before it is delivered, not merely read.
--
-- `undelivered()` selected the queue and committed, then the worker walked it: two replicas both
-- read the same rows and the phone buzzed twice for one alert. The scheduler and the webhook outbox
-- both claim with a lease; this is the third queue and the last one without it.
--
-- The lease is `next_attempt_at` rather than a new "locked_until" column, because the retry path
-- needs the same field: a failed send schedules its next attempt here, and a worker that dies
-- mid-send leaves a lease that simply expires.

ALTER TABLE notifications
    ADD COLUMN IF NOT EXISTS next_attempt_at timestamptz NOT NULL DEFAULT now();

CREATE INDEX IF NOT EXISTS notifications_push_due_idx
    ON notifications (next_attempt_at)
    WHERE delivered_at IS NULL;
