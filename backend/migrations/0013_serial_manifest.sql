-- The manufacturing manifest: the serials a vendor says it built.
--
-- Claiming used to fall back to trust-on-first-use, which means anyone holding a
-- pairing ticket could invent a serial and get a device row. That is acceptable
-- for a prototype and unacceptable for a product: the manifest is what makes
-- "this serial exists" a fact rather than a claim.
CREATE TABLE product_serials (
    vendor_id       uuid NOT NULL,
    product_id      uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    serial          text NOT NULL,
    manufactured_at timestamptz NOT NULL DEFAULT now(),
    -- Set when a device claims this serial. NULL means still on the shelf.
    claimed_at      timestamptz,
    device_id       uuid,
    PRIMARY KEY (product_id, serial)
);

-- The vendor's question is "how many of these are left", which is this index.
CREATE INDEX product_serials_open_idx
    ON product_serials (product_id, manufactured_at)
    WHERE claimed_at IS NULL;

ALTER TABLE product_serials ENABLE ROW LEVEL SECURITY;
ALTER TABLE product_serials FORCE ROW LEVEL SECURITY;

CREATE POLICY vendor_scope ON product_serials
    USING (vendor_id = haven_vendor());
