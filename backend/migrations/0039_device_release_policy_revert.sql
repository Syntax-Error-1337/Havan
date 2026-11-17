-- Undo 0038: row-level security cannot express a release, and pretending it can was the wrong lesson.
--
-- 0038 added a WITH CHECK that allows `home_id IS NULL`, on the theory that a release was a policy
-- question. It is not: for an UPDATE, PostgreSQL applies the USING expression to the *new* row as well,
-- and the USING expression has to say "you are a member of this device's home" — which a row with no home
-- cannot satisfy. Every variation tested (split by command, WITH CHECK true, USING OR'd with NULL) still
-- failed with `42501`, because the old row's membership is the only membership there is.
--
-- So the policy goes back to what it was, and the release is made where the authority actually lives: the
-- service checks that the caller is an admin of the home (a product rule, tested), and then the write runs
-- in a system context because it *changes the row's tenancy* — precisely the operation RLS exists to stop
-- an unprivileged writer from performing.

DROP POLICY home_member_scope ON devices;

CREATE POLICY home_member_scope ON devices FOR ALL
    USING (
        home_id IS NOT NULL
        AND haven_user() IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM home_members m
            WHERE m.home_id = devices.home_id AND m.user_id = haven_user()
        )
    );
