-- 0006 — a pairing ticket belongs to a home, not to a vendor.
--
-- The app mints a ticket before it knows which vendor's product the customer
-- will tap in the radar, so there is no vendor to record at mint time.

ALTER TABLE pairing_tickets ALTER COLUMN vendor_id DROP NOT NULL;

-- The hash of the ticket's nonce is not stored in v1 (the signature is the
-- proof); make that column optional rather than write a placeholder.
ALTER TABLE pairing_tickets ALTER COLUMN nonce_hash DROP NOT NULL;
