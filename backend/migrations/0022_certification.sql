-- Certification: a product may not be claimed in our app until we have signed it off.
--
-- The business is one app and many vendors, which means a vendor's unfinished device would
-- otherwise surface in *our* consumer app under *our* name. Publishing says the vendor is
-- done editing; certification says we have tested the thing.
--
--   uncertified -> submitted -> certified -> revoked
--        ^                          |
--        +--------------------------+
--
-- The states are a column and not a boolean because the interesting questions are "who
-- asked", "who signed and when", and "what did we revoke", and a boolean cannot answer them.
ALTER TABLE products ADD COLUMN IF NOT EXISTS certification text NOT NULL DEFAULT 'uncertified'
    CHECK (certification IN ('uncertified','submitted','certified','revoked'));

ALTER TABLE products ADD COLUMN IF NOT EXISTS certification_note text;
ALTER TABLE products ADD COLUMN IF NOT EXISTS certified_at timestamptz;

-- Products that are already published were shipped under the old rule, where publishing was
-- the only gate. Leaving them uncertified would take devices that work today out of the app,
-- so they are grandfathered as certified, with a note saying exactly that.
UPDATE products
   SET certification = 'certified',
       certified_at = COALESCE(published_at, now()),
       certification_note = 'grandfathered: published before the certification gate existed'
 WHERE status = 'published' AND certification = 'uncertified';

CREATE INDEX IF NOT EXISTS products_certification_idx
    ON products (certification)
    WHERE certification <> 'certified';

COMMENT ON COLUMN products.certification IS
    'Operator sign-off. A product that is not certified cannot be claimed in the app.';
