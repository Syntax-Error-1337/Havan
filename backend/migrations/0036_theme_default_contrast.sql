-- The platform's default accent moves two steps deeper.
--
-- The shipped default was system blue `#0A84FF`. White on it measures 3.65:1, so every filled button
-- in the app was below the AA floor for its own label, and the palette gate in `make check` now says
-- so. `#146EF5` is the same hue at 4.59:1 against white and still 4.1:1 against the night background.
--
-- Guarded, because a migration must never overwrite a decision an operator made: only the row that
-- is still the untouched system default is updated.

UPDATE app_theme
SET accent = '#146EF5',
    updated_at = now(),
    updated_by = 'system:contrast'
WHERE id = 1
  AND accent = '#0A84FF'
  AND updated_by = 'system';
