-- Platform appearance: one row, set by the operator, read by every client.
--
-- This is the white-label seam. The app's colours were a constant in `tokens.ts`, which meant a new
-- operator meant a new build. They are data now: the operator sets an accent, a mode and a product
-- name in the console and every phone picks it up on its next start.
--
-- Deliberately *not* a tenant table: it carries no `vendor_id` and no RLS policy, because there is
-- one platform brand and every signed-in person and vendor sees the same one. A vendor that wants
-- its own brand inside a customer's home is a different feature with a different question (whose
-- brand wins on a shared home), and it is not sneaked in here.

CREATE TABLE IF NOT EXISTS app_theme (
    id              smallint PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    product_name    text NOT NULL,
    logo_text       text NOT NULL,
    accent          text NOT NULL,
    accent_secondary text NOT NULL,
    mode            text NOT NULL CHECK (mode IN ('dark', 'light')),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    updated_by      text
);

-- Every change keeps its previous value: a brand colour that changed overnight is a question
-- ("who changed it, and what was it before") that a single row cannot answer.
CREATE TABLE IF NOT EXISTS app_theme_history (
    id          uuid PRIMARY KEY,
    changed_at  timestamptz NOT NULL DEFAULT now(),
    changed_by  text NOT NULL,
    previous    jsonb NOT NULL,
    next        jsonb NOT NULL
);

CREATE INDEX IF NOT EXISTS app_theme_history_time_idx ON app_theme_history (changed_at DESC);

INSERT INTO app_theme (id, product_name, logo_text, accent, accent_secondary, mode, updated_by)
VALUES (1, 'Haven', 'HV', '#0A84FF', '#8B5CF6', 'dark', 'system')
ON CONFLICT (id) DO NOTHING;
