-- Who to ask when the app cannot help.
--
-- Settings → About shows the version and the licences and nothing else: a customer whose device will not pair has no
-- address to write to and no place to check whether Haven is down, inside the product. Support details are the operator's,
-- not the build's, for exactly the reason the colours are: one white-label deployment, one place to change them.
--
-- Both nullable, because a deployment that has not set them must not show an empty row promising a contact that does not
-- exist.
ALTER TABLE app_theme
    ADD COLUMN support_email text,
    ADD COLUMN status_url    text;

COMMENT ON COLUMN app_theme.support_email IS
    'Where a customer writes when the app cannot help. Shown in About; absent means the row is not drawn.';
COMMENT ON COLUMN app_theme.status_url IS
    'The operator''s status page. https only: the app opens it in the system browser, and a link that could be anything is a link that could be javascript:.';
