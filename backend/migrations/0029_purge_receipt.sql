-- A receipt for the deletion itself.
--
-- `data_requests.kind` allowed 'export' and 'erase', which covers what a person asks for and not
-- what happens to their data afterwards. When the retention window passes, the tombstone is deleted
-- and something has to survive to say so — otherwise the only record of an erasure is a log line
-- that rotates away, and "we deleted it on the 4th" becomes unanswerable three months later.

ALTER TABLE data_requests DROP CONSTRAINT data_requests_kind_check;

ALTER TABLE data_requests
    ADD CONSTRAINT data_requests_kind_check
        CHECK (kind = ANY (ARRAY['export'::text, 'erase'::text, 'purge'::text]));

COMMENT ON CONSTRAINT data_requests_kind_check ON data_requests IS
    'export: a copy was produced. erase: the person was anonymised. purge: the retention window passed and the tombstone was deleted.';
