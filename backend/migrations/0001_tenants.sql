-- 0001 — tenants, vendor accounts, the catalog.
--
-- Tenancy: a vendor owns products, firmware and its own users. Customer data
-- (users, homes) is platform-wide by design: one app, many vendors.

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- The only role allowed to cross tenants. Used by retention and fan-out jobs.
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'haven_system') THEN
    CREATE ROLE haven_system NOLOGIN BYPASSRLS;
  END IF;
END
$$;

CREATE TABLE vendors (
    id             uuid PRIMARY KEY,
    slug           text NOT NULL UNIQUE CHECK (slug ~ '^[a-z0-9][a-z0-9-]{1,31}$'),
    name           text NOT NULL,
    status         text NOT NULL DEFAULT 'active'
                   CHECK (status IN ('active','suspended','deleted')),
    quota_devices  integer NOT NULL DEFAULT 10000,
    branding       jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at     timestamptz NOT NULL DEFAULT now(),
    deleted_at     timestamptz
);

CREATE TABLE vendor_users (
    id            uuid PRIMARY KEY,
    vendor_id     uuid NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    email         text NOT NULL,
    display_name  text NOT NULL,
    role          text NOT NULL CHECK (role IN ('owner','admin','developer','viewer')),
    password_hash text NOT NULL,
    totp_secret   text,
    last_login_at timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now(),
    UNIQUE (vendor_id, email)
);

CREATE TABLE api_keys (
    id           uuid PRIMARY KEY,
    vendor_id    uuid NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    name         text NOT NULL,
    prefix       text NOT NULL UNIQUE,
    hash         text NOT NULL,
    scopes       text[] NOT NULL DEFAULT '{}',
    last_used_at timestamptz,
    created_at   timestamptz NOT NULL DEFAULT now(),
    revoked_at   timestamptz
);

CREATE TABLE products (
    id             uuid PRIMARY KEY,
    vendor_id      uuid NOT NULL REFERENCES vendors(id) ON DELETE CASCADE,
    product_key    text NOT NULL UNIQUE,
    name           text NOT NULL,
    category       text NOT NULL,
    icon           text,
    thing_model    jsonb NOT NULL,
    schema_version integer NOT NULL DEFAULT 1,
    status         text NOT NULL DEFAULT 'draft'
                   CHECK (status IN ('draft','published','deprecated')),
    image_url      text,
    created_at     timestamptz NOT NULL DEFAULT now(),
    published_at   timestamptz
);

CREATE INDEX products_vendor_idx ON products (vendor_id, created_at DESC);

CREATE TABLE firmware (
    id          uuid PRIMARY KEY,
    product_id  uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    version     text NOT NULL,
    size        bigint NOT NULL,
    sha256      text NOT NULL,
    ed25519_sig text NOT NULL,
    object_key  text NOT NULL,
    status      text NOT NULL DEFAULT 'uploaded'
                CHECK (status IN ('uploaded','signed','revoked')),
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (product_id, version)
);
