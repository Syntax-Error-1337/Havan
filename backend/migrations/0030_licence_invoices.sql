-- An invoice is a frozen fact, not a query.
--
-- The ledger itself must stay live: `licence_activations` is the source of truth for what has been
-- charged, and a preview built from it is always current. But a document that a vendor pays against
-- cannot change after it was sent — a late activation, a corrected grant or a timezone edge moving a
-- device from one month to the next would silently rewrite a bill somebody already approved.
--
-- So issuing copies the period into rows and keeps the original query as the draft. The lines are
-- the invoice: quantity and amount are derived from them on issue, never entered by hand.

CREATE TABLE licence_invoices (
    id           uuid PRIMARY KEY,
    vendor_id    uuid NOT NULL REFERENCES vendors (id) ON DELETE CASCADE,
    -- `YYYY-MM`, in UTC. A text period rather than a date range because it is what a vendor asks
    -- for and what a finance system keys on; the bounds are computed from it.
    period       text NOT NULL,
    currency     text NOT NULL,
    quantity     integer NOT NULL,
    amount_cents bigint NOT NULL,
    issued_at    timestamptz NOT NULL DEFAULT now(),
    issued_by    uuid,
    CONSTRAINT licence_invoices_period_check CHECK (period ~ '^[0-9]{4}-(0[1-9]|1[0-2])$'),
    CONSTRAINT licence_invoices_unique_period UNIQUE (vendor_id, period)
);

CREATE TABLE licence_invoice_lines (
    invoice_id       uuid NOT NULL REFERENCES licence_invoices (id) ON DELETE CASCADE,
    vendor_id        uuid NOT NULL,
    serial           text NOT NULL,
    product_key      text NOT NULL,
    unit_price_cents integer NOT NULL,
    activated_at     timestamptz NOT NULL,
    -- One device bills once. This is the same rule as `licence_activations`, restated where a
    -- human reads the invoice, so a duplicate cannot appear between the ledger and the document.
    PRIMARY KEY (invoice_id, serial)
);

CREATE INDEX licence_invoice_lines_invoice_idx ON licence_invoice_lines (invoice_id);

ALTER TABLE licence_invoices ENABLE ROW LEVEL SECURITY;
ALTER TABLE licence_invoices FORCE ROW LEVEL SECURITY;
ALTER TABLE licence_invoice_lines ENABLE ROW LEVEL SECURITY;
ALTER TABLE licence_invoice_lines FORCE ROW LEVEL SECURITY;

CREATE POLICY licence_invoices_vendor ON licence_invoices
    USING (vendor_id = current_setting('haven.vendor_id', true)::uuid)
    WITH CHECK (vendor_id = current_setting('haven.vendor_id', true)::uuid);

CREATE POLICY licence_invoice_lines_vendor ON licence_invoice_lines
    USING (vendor_id = current_setting('haven.vendor_id', true)::uuid)
    WITH CHECK (vendor_id = current_setting('haven.vendor_id', true)::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON licence_invoices, licence_invoice_lines TO haven_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON licence_invoices, licence_invoice_lines TO haven_system;
