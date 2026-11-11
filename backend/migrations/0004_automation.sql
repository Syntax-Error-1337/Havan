-- 0004 — automations and notifications.

CREATE TABLE automations (
    id          uuid PRIMARY KEY,
    home_id     uuid NOT NULL REFERENCES homes(id) ON DELETE CASCADE,
    name        text NOT NULL,
    enabled     boolean NOT NULL DEFAULT true,
    definition  jsonb NOT NULL,
    next_run_at timestamptz,
    lease_until timestamptz,
    lease_owner text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX automations_home_idx ON automations (home_id, created_at DESC);
CREATE INDEX automations_due_idx ON automations (next_run_at)
    WHERE enabled AND next_run_at IS NOT NULL;

CREATE TABLE automation_runs (
    id            uuid PRIMARY KEY,
    automation_id uuid NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
    trigger_kind  text NOT NULL,
    status        text NOT NULL DEFAULT 'running'
                  CHECK (status IN ('running','completed','failed','skipped')),
    log           jsonb NOT NULL DEFAULT '[]'::jsonb,
    started_at    timestamptz NOT NULL DEFAULT now(),
    finished_at   timestamptz
);

CREATE INDEX automation_runs_idx ON automation_runs (automation_id, started_at DESC);

CREATE TABLE notifications (
    id         uuid PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind       text NOT NULL,
    title      text NOT NULL,
    body       text NOT NULL DEFAULT '',
    data       jsonb NOT NULL DEFAULT '{}'::jsonb,
    read_at    timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX notifications_user_idx ON notifications (user_id, created_at DESC);

CREATE TABLE push_tokens (
    id          uuid PRIMARY KEY,
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    platform    text NOT NULL CHECK (platform IN ('ios','android','web')),
    token       text NOT NULL,
    app_version text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, token)
);
