-- Device licences: the revenue line.
--
-- The business is not a subscription. A vendor pays per device that goes live in our app,
-- and the billable moment is the one transition the platform already performs: a serial
-- being taken out of the manufacturing manifest. `product_serials.claimed_at IS NULL`
-- means "manufactured, not yet billable"; setting it is what the customer's phone causes
-- when it pairs a device.
--
-- Two tables, because "what a vendor bought" and "what a vendor used" are different facts
-- that must not be able to disagree:
--
--   licence_grants       — the operator authorised N activations at a price. Money in.
--   licence_activations  — one row per device ever activated. Money owed.
--
-- The balance is derived (`grants - activations`) rather than stored. A stored counter is
-- a number that can drift from the rows it summarises, and the first time it drifts a
-- vendor is billed for a device that does not exist.
--
-- Charging on activation rather than on manifest upload is the fair-reading of the deal:
-- a vendor who manufactures a run and sells half of it owes for half. It also means the
-- manifest keeps growing as the factory runs, which is a manufacturing fact, not an event
-- anyone should be invoiced for.

CREATE TABLE licence_grants (
    id               uuid PRIMARY KEY,
    vendor_id        uuid NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    quantity         integer NOT NULL CHECK (quantity > 0),
    unit_price_cents integer NOT NULL CHECK (unit_price_cents >= 0),
    currency         text NOT NULL DEFAULT 'USD' CHECK (char_length(currency) = 3),
    granted_at       timestamptz NOT NULL DEFAULT now(),
    -- A grant can expire (a trial, a promotional pack). Null means it never does.
    expires_at       timestamptz,
    note             text
);

CREATE INDEX licence_grants_vendor_idx ON licence_grants (vendor_id, granted_at DESC);

CREATE TABLE licence_activations (
    id               uuid PRIMARY KEY,
    vendor_id        uuid NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    product_id       uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    serial           text NOT NULL,
    -- Written once the device row exists; null for the few milliseconds between the
    -- serial being taken and the device being inserted, and for products that allow
    -- unknown serials (trust on first use) where no manifest row exists at all.
    device_id        uuid REFERENCES devices(id) ON DELETE SET NULL,
    activated_at     timestamptz NOT NULL DEFAULT now(),
    period           date NOT NULL DEFAULT (date_trunc('month', now()))::date,
    -- The price at the moment of sale, copied rather than joined: a later price change
    -- must not rewrite an invoice that has already gone out.
    unit_price_cents integer NOT NULL DEFAULT 0,
    currency         text NOT NULL DEFAULT 'USD',

    -- Billing is idempotent per device, not per request. A claimed device that retries
    -- its claim, or a request that is replayed by a proxy, must produce one charge.
    UNIQUE (product_id, serial)
);

CREATE INDEX licence_activations_vendor_time_idx
    ON licence_activations (vendor_id, activated_at DESC);

-- The invoice query: one vendor, one month.
CREATE INDEX licence_activations_period_idx
    ON licence_activations (vendor_id, period);

ALTER TABLE licence_grants      ENABLE ROW LEVEL SECURITY;
ALTER TABLE licence_grants      FORCE ROW LEVEL SECURITY;
ALTER TABLE licence_activations ENABLE ROW LEVEL SECURITY;
ALTER TABLE licence_activations FORCE ROW LEVEL SECURITY;

-- A vendor reads its own balance and its own activations. It cannot grant itself
-- anything: the claim path writes activations as `haven_system`, and the grant path is
-- operator-only, so no vendor-scoped connection can insert into either table.
CREATE POLICY vendor_scope ON licence_grants FOR ALL
    USING (vendor_id = haven_vendor());

CREATE POLICY vendor_scope ON licence_activations FOR ALL
    USING (vendor_id = haven_vendor());

COMMENT ON TABLE licence_grants IS
    'Activations an operator authorised for a vendor. Balance = grants - activations.';
COMMENT ON TABLE licence_activations IS
    'One row per activated device: the invoice source of truth. Unique per (product, serial).';
COMMENT ON COLUMN licence_activations.period IS
    'Month the activation is invoiced in, from activated_at. Never edited.';
