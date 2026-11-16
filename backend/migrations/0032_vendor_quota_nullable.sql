-- A ceiling that can be absent.
--
-- `quota_devices` was `integer NOT NULL DEFAULT 10000`, which makes "no agreed ceiling" impossible
-- to express: every vendor silently carries a number nobody chose, and an operator clearing a quota
-- from the console would have to invent one. Null means what it should mean — no ceiling — and the
-- figure that actually limits activations is the licence ledger.
--
-- The default is dropped with the constraint so a new vendor starts unconstrained rather than at a
-- number that looks deliberate.

ALTER TABLE vendors ALTER COLUMN quota_devices DROP NOT NULL;
ALTER TABLE vendors ALTER COLUMN quota_devices DROP DEFAULT;

COMMENT ON COLUMN vendors.quota_devices IS
    'An agreed device ceiling, or NULL for none. Not enforced yet: licences are what bound activations.';
