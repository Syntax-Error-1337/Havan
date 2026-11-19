-- Quiet hours: the window in which a person does not want their phone to buzz.
--
-- The app deliberately had no such control, because the platform had nothing to enforce: a toggle over a phone that still
-- buzzes at 03:00 is worse than no toggle at all. The enforcement lives in the push query (`undelivered`), so a held
-- notification is *held*, not dropped — it goes out when the window ends, and nothing is lost in between.
--
-- Wall-clock times in the person's own zone, stored as `time` and an IANA name. Offsets were the alternative and they are
-- wrong twice a year: a 22:00 quiet hour written as `+05:30` still fires at 22:00 in a zone that has just changed.
ALTER TABLE users
    ADD COLUMN quiet_from time,
    ADD COLUMN quiet_to   time,
    -- Nullable as a trio: a window without a zone cannot be evaluated, so all three are set or none are.
    ADD COLUMN quiet_tz   text;

-- A window that starts and ends at the same minute is not "all day" or "never" — it is ambiguous, so the API refuses it.
-- What the database can check is that a zone, if present, is one the platform knows; Postgres ships the authority.
ALTER TABLE users
    ADD CONSTRAINT users_quiet_together
    CHECK ((quiet_from IS NULL) = (quiet_to IS NULL) AND (quiet_from IS NULL) = (quiet_tz IS NULL));
