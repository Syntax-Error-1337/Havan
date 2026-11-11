-- Runs once, as the superuser, when the container initialises its volume.
--
-- Why this exists: the app must NOT connect as the table owner or a superuser,
-- because both bypass row-level security. `haven_app` is an ordinary login that
-- every policy applies to; `haven_system` is the only role allowed to cross
-- tenants (BYPASSRLS) and is reachable only via SET ROLE from a system context.
--
-- Dev credentials only. In a deployed environment these roles are provisioned
-- by the platform's migration job with passwords from the secret store.

DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'haven_app') THEN
    CREATE ROLE haven_app LOGIN PASSWORD 'haven_app';
  END IF;
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'haven_system') THEN
    CREATE ROLE haven_system NOLOGIN BYPASSRLS;
  END IF;
END
$$;

GRANT haven_system TO haven_app;

GRANT USAGE ON SCHEMA public TO haven_app, haven_system;

-- Everything the application does is DML; DDL belongs to the migrate job.
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO haven_app, haven_system;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO haven_app, haven_system;

-- Tables the migrations create later must be reachable too.
ALTER DEFAULT PRIVILEGES FOR ROLE haven IN SCHEMA public
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO haven_app, haven_system;
ALTER DEFAULT PRIVILEGES FOR ROLE haven IN SCHEMA public
  GRANT USAGE, SELECT ON SEQUENCES TO haven_app, haven_system;
