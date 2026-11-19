-- Undoing a removal.
--
-- Removing a device from a home was one-way: `unclaim` cleared `home_id` and set `state = 'released'`, and the only route
-- back was pairing it again — the factory reset, the QR code, the ticket. For a mis-tap on the one destructive action in
-- the app that is a bad trade, and it is a trade the app never told anybody about: the confirm said "to use it again, pair
-- it again", which was true and was the whole problem.
--
-- The device row already survives a removal (it is `released`, not deleted), so what was missing is the two facts needed
-- to put it back: which home and which room it came from. That is this table, one row per removal, with `restored_at`
-- stamped when it is used. It doubles as the record of who removed what, which the audit trail otherwise only knows as a
-- state change.
CREATE TABLE device_releases (
    id          uuid PRIMARY KEY,
    device_id   uuid NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    vendor_id   uuid NOT NULL,
    home_id     uuid NOT NULL,
    room_id     uuid,
    released_by uuid NOT NULL,
    released_at timestamptz NOT NULL DEFAULT now(),
    restored_at timestamptz
);

-- The restore lookup is "the latest removal of this device", and it must be an index read rather than a scan: a device
-- that is removed and re-paired a hundred times (a factory bench does exactly that) leaves a hundred rows behind.
CREATE INDEX device_releases_device_idx ON device_releases (device_id, released_at DESC);

ALTER TABLE device_releases ENABLE ROW LEVEL SECURITY;
ALTER TABLE device_releases FORCE ROW LEVEL SECURITY;

-- Scoped like every other tenant table. The restore itself runs in the system context, after the domain layer has checked
-- that the caller belongs to the home the device came from — the policy is the floor, not the check.
CREATE POLICY device_releases_scope ON device_releases FOR ALL
    USING (vendor_id = haven_vendor())
    WITH CHECK (vendor_id = haven_vendor());
