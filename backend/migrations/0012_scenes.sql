-- Scenes: a named bundle of actions a person presses.
--
-- Deliberately separate from `automations`: a scene has no trigger, no
-- conditions and no schedule. It is the "good night" button, and giving it a
-- trigger column would invite people to build automations out of scenes and end
-- up with two engines.
CREATE TABLE scenes (
    id         uuid PRIMARY KEY,
    home_id    uuid NOT NULL REFERENCES homes(id) ON DELETE CASCADE,
    name       text NOT NULL,
    icon       text,
    actions    jsonb NOT NULL DEFAULT '[]'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX scenes_home_idx ON scenes (home_id, created_at DESC);

ALTER TABLE scenes ENABLE ROW LEVEL SECURITY;
ALTER TABLE scenes FORCE ROW LEVEL SECURITY;

-- Any member of the home may see and run a scene; that is what a scene is for.
CREATE POLICY home_member_scope ON scenes
    USING (
        haven_user() IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM home_members m
             WHERE m.home_id = scenes.home_id AND m.user_id = haven_user()
        )
    );
