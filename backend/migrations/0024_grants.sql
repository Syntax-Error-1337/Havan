-- Privileges for tables added after the cluster was set up.
--
-- The development database has default privileges for `haven_app` and `haven_system` on anything
-- created in `public`; the test database does not, because it was created by `make testdb` and
-- migrated separately. So a new table worked in development and failed in the suite with
-- "permission denied for table data_requests" — a failure that looks like a code bug and is a
-- missing GRANT.
--
-- Both halves are needed: the explicit grant repairs the table that already exists, and the default
-- privilege stops the next migration from reintroducing this.
GRANT SELECT, INSERT, UPDATE, DELETE ON data_requests TO haven_app, haven_system;

ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO haven_app, haven_system;

COMMENT ON TABLE data_requests IS
    'Append-only receipt of export/erasure requests. No FK: kept after the account is erased.';
