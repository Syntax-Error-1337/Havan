-- Outbound webhooks: a registry and an outbox.
--
-- The outbox exists so a vendor's endpoint is never in the path of a customer's pairing. The event
-- row is written in the *same transaction* as the fact it describes — an activation and its
-- `licence.activated` either both exist or neither does — and a worker delivers it afterwards.
--
-- `attempts` and `next_attempt_at` make a failing endpoint the vendor's problem, bounded: after
-- `MAX_ATTEMPTS` the row stops being retried and the last error is kept for them to read.
CREATE TABLE vendor_webhooks (
    id         uuid PRIMARY KEY,
    vendor_id  uuid NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    url        text NOT NULL,
    -- Needed in the clear to compute each signature: this is a shared secret, not a password.
    secret     text NOT NULL,
    events     text[] NOT NULL,
    active     boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (vendor_id, url)
);

ALTER TABLE vendor_webhooks ENABLE ROW LEVEL SECURITY;
ALTER TABLE vendor_webhooks FORCE ROW LEVEL SECURITY;
CREATE POLICY vendor_scope ON vendor_webhooks FOR ALL
    USING (vendor_id = haven_vendor());

CREATE TABLE webhook_outbox (
    id              uuid PRIMARY KEY,
    vendor_id       uuid NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    event           text NOT NULL,
    payload         jsonb NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    delivered_at    timestamptz,
    attempts        integer NOT NULL DEFAULT 0,
    last_error      text,
    next_attempt_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX webhook_outbox_due_idx
    ON webhook_outbox (next_attempt_at)
    WHERE delivered_at IS NULL;

ALTER TABLE webhook_outbox ENABLE ROW LEVEL SECURITY;
ALTER TABLE webhook_outbox FORCE ROW LEVEL SECURITY;
CREATE POLICY vendor_scope ON webhook_outbox FOR ALL
    USING (vendor_id = haven_vendor());

COMMENT ON TABLE webhook_outbox IS
    'Transactional outbox for vendor webhooks: written with the fact, delivered by a worker.';
COMMENT ON COLUMN vendor_webhooks.secret IS
    'Shared signing secret, shown once at creation. Not a password: it must stay readable to sign.';
