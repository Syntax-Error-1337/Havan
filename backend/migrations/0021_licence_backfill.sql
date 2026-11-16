-- Backfill: every device that already exists was activated before there was a bill.
--
-- The ledger started empty while one vendor already had a device in a customer's home, so
-- the first invoice would have been wrong by exactly the fleet. `ON CONFLICT DO NOTHING`
-- keeps this safe to re-run and safe against the constraint that already makes billing
-- idempotent.
--
-- Released devices are skipped: they are out of the homes they were activated into, and a
-- vendor should not owe for a unit that came back.
--
-- The price is left at zero on purpose. These activations happened before a price existed,
-- and inventing one would put a number on an invoice that nobody agreed to. An operator
-- covers them with a grant, which is exactly the conversation a vendor should have.

INSERT INTO licence_activations (id, vendor_id, product_id, serial, device_id, activated_at, period)
SELECT gen_random_uuid(),
       d.vendor_id,
       d.product_id,
       d.serial,
       d.id,
       COALESCE(d.claimed_at, d.created_at),
       (date_trunc('month', COALESCE(d.claimed_at, d.created_at)))::date
  FROM devices d
 WHERE d.state <> 'released'
ON CONFLICT (product_id, serial) DO NOTHING;
