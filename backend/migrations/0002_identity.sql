-- 0002 — people, homes, rooms.
--
-- A customer is a platform user, not a vendor's user: the whole point is that
-- one app holds devices from many vendors.

CREATE TABLE users (
    id            uuid PRIMARY KEY,
    email         text UNIQUE,
    phone         text UNIQUE,
    password_hash text,
    display_name  text NOT NULL DEFAULT '',
    locale        text NOT NULL DEFAULT 'en',
    avatar_url    text,
    totp_secret   text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    disabled_at   timestamptz,
    CHECK (email IS NOT NULL OR phone IS NOT NULL)
);

CREATE TABLE homes (
    id         uuid PRIMARY KEY,
    owner_id   uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name       text NOT NULL,
    timezone   text NOT NULL DEFAULT 'UTC',
    geo_lat    double precision,
    geo_lon    double precision,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX homes_owner_idx ON homes (owner_id);

CREATE TABLE home_members (
    home_id    uuid NOT NULL REFERENCES homes(id) ON DELETE CASCADE,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role       text NOT NULL CHECK (role IN ('owner','admin','member','guest')),
    can_pair   boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (home_id, user_id)
);

CREATE INDEX home_members_user_idx ON home_members (user_id);

CREATE TABLE rooms (
    id         uuid PRIMARY KEY,
    home_id    uuid NOT NULL REFERENCES homes(id) ON DELETE CASCADE,
    name       text NOT NULL,
    icon       text,
    sort       integer NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX rooms_home_idx ON rooms (home_id, sort);

CREATE TABLE refresh_tokens (
    id         uuid PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    family_id  uuid NOT NULL,
    hash       text NOT NULL,
    expires_at timestamptz NOT NULL,
    used_at    timestamptz,
    revoked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX refresh_tokens_family_idx ON refresh_tokens (family_id);
CREATE INDEX refresh_tokens_user_idx ON refresh_tokens (user_id, expires_at DESC);
