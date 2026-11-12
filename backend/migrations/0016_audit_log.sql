-- Audit log: who did what, in a table nobody can rewrite.
--
-- A vendor portal that lets somebody publish a product, revoke a key or start a
-- rollout and keeps no record of who did it is a portal where an incident cannot be
-- explained. This table is append-only by construction: there is no UPDATE or DELETE
-- path in the code, and the policy grants INSERT and SELECT only.

CREATE TABLE audit_log (
    id           uuid PRIMARY KEY,
    ts           timestamptz NOT NULL DEFAULT now(),
    -- Every row belongs to one vendor. System actions (a worker closing a campaign)
    -- carry the vendor too, so a vendor sees its own automation and nobody else's.
    vendor_id    uuid NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    -- 'user' (a person in the portal), 'api_key' (a vendor's server), 'system' (a job).
    actor_kind   text NOT NULL,
    actor_id     uuid,
    -- A verb in the past tense: 'product.published', 'api_key.revoked'.
    action       text NOT NULL,
    target_kind  text NOT NULL,
    target_id    uuid,
    request_id   text,
    ip           inet,
    -- Anything else worth keeping, as data rather than columns: an audit trail that
    -- needs a migration for every new event is an audit trail with gaps.
    detail       jsonb NOT NULL DEFAULT '{}'::jsonb
);

-- The only query the portal makes: newest first, for one vendor.
CREATE INDEX audit_log_vendor_time_idx ON audit_log (vendor_id, ts DESC);

ALTER TABLE audit_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_log FORCE ROW LEVEL SECURITY;

CREATE POLICY vendor_scope ON audit_log
    USING (vendor_id = haven_vendor());

-- Writes come from the API in a vendor context; the policy above covers that. There is
-- deliberately no UPDATE or DELETE grant path: an audit row that can be edited is not
-- evidence.
COMMENT ON TABLE audit_log IS
    'Append-only: written by the API on administrative actions, read by the vendor portal.';
