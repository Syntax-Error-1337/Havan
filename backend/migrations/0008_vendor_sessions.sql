-- 0008 — a session belongs to either a customer or a vendor, never both.
--
-- The vendor rides on the refresh row so a refresh cannot upgrade a customer
-- token into a vendor scope: the scope comes from the stored session, not from
-- anything the client sends.

ALTER TABLE refresh_tokens ADD COLUMN vendor_id uuid REFERENCES vendors(id) ON DELETE CASCADE;

CREATE INDEX refresh_tokens_vendor_idx ON refresh_tokens (vendor_id) WHERE vendor_id IS NOT NULL;
