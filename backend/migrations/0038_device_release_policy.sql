-- Releasing a device: the policy that made it impossible.
--
-- `home_member_scope` was written with a single USING expression, which Postgres applies to the *new*
-- row as well. Every release sets `home_id = NULL`, and the new row could never satisfy "is a member of
-- home_id" — so `DELETE /v1/devices/{id}` failed with `42501 new row violates row-level security policy`
-- and the API turned that into `503 unavailable`. The route was documented, the app could reach it, and
-- the operation could not succeed for anybody.
--
-- The fix is to say what each side means separately:
--   * USING       — you may only touch a device you can already see: a member of the home it is in.
--   * WITH CHECK  — the row you leave behind is either still in a home you belong to, or in no home at
--                   all (released). A device cannot be moved into someone else's home this way, because
--                   the new home is not a home you are a member of.
--
-- Who may release is a separate question and stays in the API (an admin of that home), because role is
-- a policy of the product, not of the database.

DROP POLICY home_member_scope ON devices;

CREATE POLICY home_member_scope ON devices FOR ALL
    USING (
        home_id IS NOT NULL
        AND haven_user() IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM home_members m
            WHERE m.home_id = devices.home_id AND m.user_id = haven_user()
        )
    )
    WITH CHECK (
        haven_user() IS NOT NULL
        AND (
            home_id IS NULL
            OR EXISTS (
                SELECT 1 FROM home_members m
                WHERE m.home_id = devices.home_id AND m.user_id = haven_user()
            )
        )
    );
