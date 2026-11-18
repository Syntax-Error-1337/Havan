-- Scenes in the order the person put them in.
--
-- The list was `ORDER BY created_at DESC`, so a scene's position was the moment it was created and could not be
-- changed: a house with eight scenes had one useful one at the bottom, and the only way to move it was to delete and
-- recreate it (which also rebuilt every action).
--
-- `sort_order` descending, so a new scene still lands at the top where it did before, and older ones keep their places.
ALTER TABLE scenes ADD COLUMN sort_order integer NOT NULL DEFAULT 0;

-- Newest first, as the list was: the newest scene gets the highest number.
WITH ranked AS (
    SELECT id,
           row_number() OVER (PARTITION BY home_id ORDER BY created_at ASC) AS position
    FROM scenes
)
UPDATE scenes s
   SET sort_order = ranked.position
  FROM ranked
 WHERE s.id = ranked.id;

CREATE INDEX scenes_order_idx ON scenes (home_id, sort_order DESC, created_at DESC);

COMMENT ON COLUMN scenes.sort_order IS
    'Position in the home''s scene list, descending: bigger is nearer the top. Rewritten densely on every move.';
