//! Devices as the app sees them: a name, a room, and the points a customer can
//! touch. Vendor-facing fleet operations live in the vendor slice.
//!
//! Authorisation is inherited from [`crate::homes`]: a device belongs to a home,
//! and only that home's members may see it — a device id is not a capability.

use std::sync::Arc;

use kernel::AppError;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::homes::{HomeRepo, HomeService, Role};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PointValue {
    Bool(bool),
    Num(f64),
    Text(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Point {
    pub id: Uuid,
    /// `<capability>.<slot>`, e.g. `brightness.level`.
    pub code: String,
    /// The vendor's datapoint id — what actually goes on the wire.
    pub dp: String,
    pub capability: String,
    pub slot: String,
    pub kind: String,
    pub writable: bool,
    pub unit: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// `off | change | interval:N | all`. Sent to clients so a screen never offers
    /// a chart for a point the platform does not store.
    pub history: Option<String>,
    pub value: Option<PointValue>,
    /// When this reading was written.
    ///
    /// Without it a client can only ask "is the *device* reachable", and a sensor that is online but has not
    /// reported a temperature for eleven hours has to be drawn as if the number were current — the exact
    /// confident-stale-number the presence work was meant to remove, one level down. The column existed; it
    /// was simply never sent.
    #[serde(with = "time::serde::rfc3339::option")]
    pub updated_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub id: Uuid,
    pub device_id: Uuid,
    pub point_code: String,
    pub op: String,
    pub status: String,
    /// False when an idempotent replay returned the command already in flight.
    pub created: bool,
}

/// Everything the transport needs to put a command on the wire.
///
/// A command is one of two things, and the wire envelope has always had both fields: a data-point set (`dp` + `value`)
/// or a named action (`action`). Only the first was ever filled in, which is why `identify` was in the API docs and in
/// the app's wishlist without existing anywhere.
#[derive(Debug, Clone)]
pub struct Publishable {
    pub vendor_id: Uuid,
    pub home_id: Uuid,
    pub dp: Option<String>,
    pub value: Option<PointValue>,
    /// A named device action — `identify`, `calibrate`. Not a point: it has no value and no last-known state.
    pub action: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CommandOutcome {
    pub command: Command,
    /// `None` when an idempotent replay returned a command already in flight.
    pub publish: Option<Publishable>,
}

/// A command that never left the process, in the shape needed to try again.
///
/// Everything is copied rather than joined later: redelivery runs in the worker, minutes
/// after the request, and must not depend on a session or a tenant scope.
#[derive(Debug, Clone)]
pub struct Redelivery {
    pub command_id: Uuid,
    pub vendor_id: Uuid,
    pub home_id: Uuid,
    pub device_id: Uuid,
    pub dp: Option<String>,
    pub value: Option<serde_json::Value>,
}

/// One row of the command log, as the worker sees it after settling.
#[derive(Debug, Clone)]
pub struct Settled {
    pub command_id: Uuid,
    pub device_id: Uuid,
    pub home_id: Option<Uuid>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: Uuid,
    /// The vendor that made it. Public: it is the brand on the box.
    pub vendor_id: Uuid,
    pub home_id: Option<Uuid>,
    pub room_id: Option<Uuid>,
    pub room_name: Option<String>,
    pub name: String,
    pub product_key: String,
    pub category: String,
    /// Printed on the box and advertised over BLE: how a phone recognises the
    /// device it is looking at, and how it tells twenty identical boxes apart.
    pub serial: String,
    pub online: bool,
    pub fw_version: Option<String>,
    /// When the platform last *heard* from it, as an RFC 3339 string.
    ///
    /// Serialised explicitly: `time`'s default for `OffsetDateTime` is a field tuple, so this arrived as
    /// `[2026,321,3,26,34,…]` and every client's `new Date(...)` produced an invalid date — which the app
    /// then rendered as "just now". A device that had been silent for four hours looked current, and no
    /// amount of UI work could fix that from the client side.
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_seen_at: Option<time::OffsetDateTime>,
    /// Seconds between expected reports, as declared by the product and copied at claim.
    ///
    /// `None` means the product never said, and the client falls back to a platform default rather than
    /// inventing an expectation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report_interval_secs: Option<i32>,
    pub points: Vec<Point>,
}

/// A page of a home's devices, plus what it takes to ask for the next one.
///
/// `next_cursor` is opaque to the client on purpose: it names a position in an ordering the server
/// owns, so the server can change the ordering without the app shipping a matching change.
/// The home's presence, as counts that add up to the fleet a customer sees.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Presence {
    pub total: i64,
    pub online: i64,
    /// Online but quiet, or not online at all: the number the header's pill shows.
    pub silent: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePage {
    pub devices: Vec<Device>,
    pub next_cursor: Option<String>,
    /// The home's device count, not the page's. A phone that shows "200 of 431" needs the 431, and a
    /// `COUNT(*)` on an indexed home is cheaper than a second round trip.
    pub total: i64,
}

/// The cursor for `list_page`, in the ordering the home screen uses: room order, then name, then id.
///
/// Encoded rather than signed because it grants nothing: it is a position in a list the caller is
/// already allowed to read, and a wrong one is refused rather than trusted.
pub fn encode_cursor(room_sort: i32, name: &str, id: Uuid) -> String {
    format!("{room_sort}:{}:{id}", hex_encode(name))
}

pub fn decode_cursor(raw: &str) -> Result<(i32, String, Uuid), AppError> {
    let mut parts = raw.split(':');
    let (Some(sort), Some(name), Some(id), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(
            AppError::validation("that cursor is not one this server issued")
                .with_detail("field", "device_cursor"),
        );
    };

    let sort = sort.parse::<i32>().map_err(|_| {
        AppError::validation("that cursor is not one this server issued")
            .with_detail("field", "device_cursor")
    })?;
    let name = hex_decode(name).ok_or_else(|| {
        AppError::validation("that cursor is not one this server issued")
            .with_detail("field", "device_cursor")
    })?;
    let id = Uuid::parse_str(id).map_err(|_| {
        AppError::validation("that cursor is not one this server issued")
            .with_detail("field", "device_cursor")
    })?;

    Ok((sort, name, id))
}

/// Hex, not base64: the cursor travels in a URL and a device name can contain anything. Hex has no
/// padding, no alphabet of its own, and cannot be mistaken for a path segment.
fn hex_encode(value: &str) -> String {
    value.bytes().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_decode(value: &str) -> Option<String> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    let bytes = (0..value.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()?;
    String::from_utf8(bytes).ok()
}

/// Every method takes the acting user: the repository runs inside a
/// transaction that carries `haven.user_id`, and the database's policy decides
/// what that user may see. A method without a user cannot be written.
pub trait DeviceRepo {
    fn list_for_home(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        search: Option<&str>,
    ) -> impl Future<Output = Result<Vec<Device>, AppError>> + Send;

    /// One page of a home's devices, in the order the home screen draws them.
    ///
    /// Defaulted so an in-memory repo in a test does not have to grow a cursor: it slices the full
    /// list, which is exactly right for three devices and is never used in production.
    /// How the home as a whole is doing: devices online, devices that have gone quiet.
    ///
    /// The header's presence pill used to count the *page* it had fetched, so a 1 200-device home read
    /// "200 offline" — a fact about the response, printed as a fact about the house. This is one indexed
    /// count over the home's own rows, and it is what the pill needs to be true at any size.
    ///
    /// "Silent" is the same rule the client applies per device (`max(15 min, 3 x report_interval)`),
    /// expressed once so the header and the tiles cannot disagree.
    fn home_presence(
        &self,
        home_id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Presence, AppError>> + Send;

    fn list_page(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        limit: i64,
        after: Option<(i32, String, Uuid)>,
    ) -> impl Future<Output = Result<DevicePage, AppError>> + Send
    where
        Self: Sync,
    {
        async move {
            let all = self.list_for_home(home_id, user_id, None).await?;
            let total = all.len() as i64;
            let start = after
                .as_ref()
                .and_then(|(_, _, id)| all.iter().position(|device| device.id == *id))
                .map(|index| index + 1)
                .unwrap_or(0);

            let devices: Vec<Device> = all
                .into_iter()
                .skip(start)
                .take(limit.max(1) as usize)
                .collect();
            let next_cursor = if (start as i64) + (devices.len() as i64) < total {
                devices
                    .last()
                    .map(|device| encode_cursor(0, &device.name, device.id))
            } else {
                None
            };

            Ok(DevicePage {
                devices,
                next_cursor,
                total,
            })
        }
    }

    fn get(
        &self,
        device_id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Option<Device>, AppError>> + Send;

    fn home_of(
        &self,
        device_id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Option<Uuid>, AppError>> + Send;

    fn rename(
        &self,
        device_id: Uuid,
        user_id: Uuid,
        name: &str,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn assign_room(
        &self,
        device_id: Uuid,
        user_id: Uuid,
        room_id: Option<Uuid>,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Clear the owner, keep the row and its history.
    fn unclaim(
        &self,
        device_id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Write down what a removal took away, so it can be given back.
    fn record_release(
        &self,
        release: &Release,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// The most recent removal of this device, if it has not been undone.
    fn latest_release(
        &self,
        device_id: Uuid,
    ) -> impl Future<Output = Result<Option<Release>, AppError>> + Send;

    /// Put a released device back into the home and room it came from.
    fn restore(
        &self,
        device_id: Uuid,
        home_id: Uuid,
        room_id: Option<Uuid>,
        user_id: Uuid,
    ) -> impl Future<Output = Result<(), AppError>> + Send;
}

/// What a removal took away.
///
/// The device row already survives a removal — it is `released`, not deleted — so the only things needed to put it back
/// are the home and the room it came from, who removed it, and when. `restored_at` is the stamp that makes a release
/// single-use: the same row cannot be replayed to drag a device out of a home it has since been paired into.
#[derive(Debug, Clone)]
pub struct Release {
    pub id: Uuid,
    pub device_id: Uuid,
    pub vendor_id: Uuid,
    pub home_id: Uuid,
    pub room_id: Option<Uuid>,
    pub released_by: Uuid,
    pub released_at: OffsetDateTime,
    pub restored_at: Option<OffsetDateTime>,
}

/// Commands live next to devices because they are authorised the same way:
/// by the membership of the home the device belongs to.
pub trait CommandRepo {
    /// Record a named action (`identify`) — no point, no value, no last-known state.
    fn insert_action_command(
        &self,
        device_id: Uuid,
        user_id: Uuid,
        action: &str,
        idempotency_key: Option<&str>,
    ) -> impl Future<Output = Result<Command, AppError>> + Send;

    fn insert_command(
        &self,
        device_id: Uuid,
        user_id: Uuid,
        point_id: Uuid,
        point_code: &str,
        payload: serde_json::Value,
        idempotency_key: Option<&str>,
    ) -> impl Future<Output = Result<Command, AppError>> + Send;

    fn find_command(
        &self,
        command_id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Option<Command>, AppError>> + Send;

    /// Settle an ack. Returns `None` for an unknown id or one already settled,
    /// so a redelivered ack cannot resurrect a timed-out command.
    fn settle_from_ack(
        &self,
        command_id: Uuid,
        ok: bool,
        reported: serde_json::Value,
        code: Option<&str>,
    ) -> impl Future<Output = Result<Option<Settled>, AppError>> + Send;

    /// Commands refused because the broker was down, recent enough that delivering them
    /// late is still what the person asked for.
    fn due_for_redelivery(
        &self,
        younger_than_secs: i64,
        limit: i64,
    ) -> impl Future<Output = Result<Vec<Redelivery>, AppError>> + Send;

    /// Claim a refused command for redelivery, atomically.
    ///
    /// This is the *first* step, not the last: marking after publishing leaves a window in
    /// which two workers both publish and one of them discovers it lost the race — the device
    /// acts twice. Claiming first means the loser never publishes at all, and a claim that
    /// fails to publish is put back for the next sweep.
    fn claim_for_redelivery(
        &self,
        command_id: Uuid,
    ) -> impl Future<Output = Result<bool, AppError>> + Send;

    /// Put a claimed command back after a failed publish, so the next sweep tries again.
    fn release_redelivery(
        &self,
        command_id: Uuid,
        reason: &str,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Mark a redelivered command settled once it is on the wire.
    fn mark_redelivered(
        &self,
        command_id: Uuid,
    ) -> impl Future<Output = Result<bool, AppError>> + Send;

    /// Settle a command that never left the process because the broker was unavailable.
    ///
    /// Recorded as failed with a reason, deliberately *not* left as queued: the timeout
    /// sweeper would later mark it `timeout` and the customer would be told the device did
    /// not answer, blaming hardware for an outage that was ours.
    fn fail_undelivered(
        &self,
        command_id: Uuid,
        reason: &str,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Anything dispatched and unanswered past the deadline becomes `timeout`.
    fn sweep_timeouts(
        &self,
        older_than_secs: i64,
    ) -> impl Future<Output = Result<Vec<Settled>, AppError>> + Send;
}

pub struct DeviceService<D, H> {
    devices: Arc<D>,
    commands: Arc<D>,
    homes: Arc<HomeService<H>>,
}

impl<D, H> DeviceService<D, H>
where
    D: DeviceRepo + Send + Sync,
    H: HomeRepo + Send + Sync,
{
    pub fn new(devices: Arc<D>, homes: Arc<HomeService<H>>) -> Self {
        Self {
            commands: devices.clone(),
            devices,
            homes,
        }
    }

    pub async fn list(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        search: Option<String>,
    ) -> Result<Vec<Device>, AppError> {
        self.homes.require(home_id, user_id).await?;
        // Sanitised here, like the fleet screen's: `%` and `_` are wildcards, and a customer typing a serial
        // with an underscore in it must not turn their search into a pattern.
        let needle = search
            .as_deref()
            .map(|raw| raw.trim().chars().take(64).collect::<String>())
            .filter(|raw| !raw.is_empty())
            .map(|raw| {
                raw.replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            });
        self.devices
            .list_for_home(home_id, user_id, needle.as_deref())
            .await
    }

    /// One page, with the home check first so a stranger learns nothing about the page size.
    /// The home's presence. A read-through: the rule lives in the repository because it is one SQL
    /// aggregate, and the service has nothing to decide.
    pub async fn home_presence(&self, home_id: Uuid, user_id: Uuid) -> Result<Presence, AppError> {
        self.devices.home_presence(home_id, user_id).await
    }

    pub async fn list_page(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        limit: i64,
        cursor: Option<&str>,
    ) -> Result<DevicePage, AppError> {
        self.homes.require(home_id, user_id).await?;

        let after = match cursor.map(str::trim).filter(|value| !value.is_empty()) {
            Some(raw) => Some(decode_cursor(raw)?),
            None => None,
        };

        // A caller asking for a million rows is asking for an outage; 500 fills any phone screen
        // several times over and bounds the work a single request can cause.
        self.devices
            .list_page(home_id, user_id, limit.clamp(1, 500), after)
            .await
    }

    pub async fn get(&self, device_id: Uuid, user_id: Uuid) -> Result<Device, AppError> {
        let home_id = self.device_home(device_id, user_id).await?;
        self.homes.require(home_id, user_id).await?;
        self.devices
            .get(device_id, user_id)
            .await?
            .ok_or_else(|| AppError::not_found("device"))
    }

    pub async fn rename(
        &self,
        device_id: Uuid,
        user_id: Uuid,
        name: &str,
    ) -> Result<Device, AppError> {
        let home = self
            .homes
            .require(self.device_home(device_id, user_id).await?, user_id)
            .await?;
        if !home.role.can_manage() {
            return Err(AppError::forbidden("only an admin can rename a device"));
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err(AppError::validation("device name must be 1–60 characters"));
        }
        self.devices.rename(device_id, user_id, name).await?;
        self.get(device_id, user_id).await
    }

    pub async fn assign_room(
        &self,
        device_id: Uuid,
        user_id: Uuid,
        room_id: Option<Uuid>,
    ) -> Result<Device, AppError> {
        let home_id = self.device_home(device_id, user_id).await?;
        let home = self.homes.require(home_id, user_id).await?;
        if !home.role.can_manage() {
            return Err(AppError::forbidden("only an admin can move a device"));
        }
        self.devices
            .assign_room(device_id, user_id, room_id)
            .await?;
        self.get(device_id, user_id).await
    }

    /// Release the device. The row survives so the vendor's fleet history and
    /// the customer's telemetry keep their foreign keys; the customer loses
    /// access because the device no longer belongs to their home.
    pub async fn unclaim(&self, device_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
        let device = self.get(device_id, user_id).await?;
        let home = self
            .homes
            .require(self.device_home(device_id, user_id).await?, user_id)
            .await?;
        if home.role < Role::Admin {
            return Err(AppError::forbidden("only an admin can remove a device"));
        }

        // Write the release down *before* the removal, and only if it is recorded: a removal nobody can undo is a
        // removal the customer is not told they can undo. If the record cannot be written the request fails here,
        // while the device is still in the home — the safe direction.
        self.devices
            .record_release(&Release {
                id: kernel::new_id(),
                device_id,
                vendor_id: device.vendor_id,
                home_id: home.id,
                room_id: device.room_id,
                released_by: user_id,
                released_at: OffsetDateTime::now_utc(),
                restored_at: None,
            })
            .await?;

        self.devices.unclaim(device_id, user_id).await
    }

    /// Put a removed device back into the home and room it came from.
    ///
    /// The window is the whole design: long enough to catch a mis-tap ("I meant the one next to it"), short enough that a
    /// device taken away deliberately and paired into another home is not dragged back out of it by a stale undo. After
    /// it, the device has to be paired again — which is what the removal always meant.
    pub async fn restore(
        &self,
        device_id: Uuid,
        user_id: Uuid,
        window: std::time::Duration,
    ) -> Result<Uuid, AppError> {
        let Some(release) = self.devices.latest_release(device_id).await? else {
            return Err(AppError::not_found(
                "that device was not removed from a home",
            ));
        };

        if release.restored_at.is_some() {
            return Err(AppError::code(
                kernel::ErrorCode::Conflict,
                "that device is already back in its home",
            ));
        }

        let age = OffsetDateTime::now_utc() - release.released_at;
        if age > time::Duration::try_from(window).unwrap_or(time::Duration::ZERO) {
            return Err(AppError::code(
                kernel::ErrorCode::Conflict,
                "too late to undo that removal — pair the device again to add it back",
            ));
        }

        // The same door as removal, in reverse: an admin of the home it came out of, and nothing else. A released device
        // belongs to no home, so this is the only authority that can name one.
        let home = self.homes.require(release.home_id, user_id).await?;
        if home.role < Role::Admin {
            return Err(AppError::forbidden("only an admin can restore a device"));
        }

        self.devices
            .restore(device_id, release.home_id, release.room_id, user_id)
            .await?;
        Ok(release.home_id)
    }

    async fn device_home(&self, device_id: Uuid, user_id: Uuid) -> Result<Uuid, AppError> {
        self.devices
            .home_of(device_id, user_id)
            .await?
            .ok_or_else(|| AppError::not_found("device"))
    }
}

impl<D, H> DeviceService<D, H>
where
    D: DeviceRepo + CommandRepo + Send + Sync,
    H: HomeRepo + Send + Sync,
{
    /// Validate and record a command. Publishing happens in the API layer —
    /// this slice must stay transport-free.
    pub async fn command(
        &self,
        device_id: Uuid,
        user_id: Uuid,
        code: &str,
        value: Option<PointValue>,
        idempotency_key: Option<String>,
    ) -> Result<CommandOutcome, AppError> {
        let device = self.get(device_id, user_id).await?;

        let home_id = device
            .home_id
            .ok_or_else(|| AppError::validation("this device is not in a home"))?;

        let point = device
            .points
            .iter()
            .find(|p| p.code == code)
            .ok_or_else(|| {
                AppError::code(
                    kernel::ErrorCode::UnknownPoint,
                    format!("unknown_point: this device has no point '{code}'"),
                )
            })?;

        if !point.writable {
            return Err(AppError::code(
                kernel::ErrorCode::ValidationFailed,
                format!("'{code}' is read-only"),
            ));
        }

        let value = value.ok_or_else(|| AppError::validation("a set command needs a value"))?;
        validate_value(point, &value)?;

        let payload = serde_json::json!({ "dp": point.dp, "value": value });

        let command = self
            .commands
            .insert_command(
                device_id,
                user_id,
                point.id,
                &point.code,
                payload,
                idempotency_key.as_deref(),
            )
            .await?;

        let publish = command.created.then(|| Publishable {
            vendor_id: device.vendor_id,
            home_id,
            dp: Some(point.dp.clone()),
            value: Some(value),
            action: None,
        });

        Ok(CommandOutcome { command, publish })
    }

    /// Ask a device to announce itself — `identify`: blink, beep, whatever the hardware can do.
    ///
    /// This is a named action rather than a data point, because there is no state to keep: nobody wants to know that a
    /// device *is* identifying, they want it to do it now. The wire envelope has carried an `action` field since the first
    /// version; nothing filled it in until here.
    ///
    /// Whether the device answers is the device's business, and the app says so: the command settles `applied`,
    /// `rejected` or `timeout` like any other, and "it did not answer" is a true sentence about a firmware that has never
    /// heard of `identify`.
    pub async fn identify(
        &self,
        device_id: Uuid,
        user_id: Uuid,
    ) -> Result<CommandOutcome, AppError> {
        let device = self.get(device_id, user_id).await?;
        let home_id = device
            .home_id
            .ok_or_else(|| AppError::validation("this device is not in a home"))?;

        let command = self
            .commands
            .insert_action_command(device_id, user_id, "identify", None)
            .await?;

        let publish = command.created.then(|| Publishable {
            vendor_id: device.vendor_id,
            home_id,
            dp: None,
            value: None,
            action: Some("identify".into()),
        });

        Ok(CommandOutcome { command, publish })
    }

    /// Settle a command the broker never received.
    ///
    /// Goes through the service rather than the repo so the API does not learn about
    /// scopes: the caller has a command id and an outage, nothing more.
    pub async fn fail_undelivered(&self, command_id: Uuid, reason: &str) -> Result<(), AppError> {
        self.commands.fail_undelivered(command_id, reason).await
    }

    pub async fn command_status(
        &self,
        command_id: Uuid,
        user_id: Uuid,
    ) -> Result<Command, AppError> {
        self.commands
            .find_command(command_id, user_id)
            .await?
            .ok_or_else(|| AppError::not_found("command"))
    }
}

/// The kind decides what a value may be; the range decides which values.
/// Both are checked here rather than at the device, because a firmware that
/// trusts the cloud will happily drive a relay past its rating.
fn validate_value(point: &Point, value: &PointValue) -> Result<(), AppError> {
    match (point.kind.as_str(), value) {
        ("bool", PointValue::Bool(_)) => Ok(()),
        ("int" | "float", PointValue::Num(n)) => {
            if let Some(min) = point.min
                && *n < min
            {
                return Err(AppError::code(
                    kernel::ErrorCode::ValueOutOfRange,
                    format!("'{}' must be at least {min}", point.code),
                ));
            }
            if let Some(max) = point.max
                && *n > max
            {
                return Err(AppError::code(
                    kernel::ErrorCode::ValueOutOfRange,
                    format!("'{}' must be at most {max}", point.code),
                ));
            }
            Ok(())
        }
        ("text" | "enum" | "raw", PointValue::Text(_)) => Ok(()),
        (kind, _) => Err(AppError::validation(format!(
            "'{}' expects a {kind} value",
            point.code
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::homes::{Home, Member, Room};
    use kernel::ErrorCode;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeHomes {
        homes: Mutex<Vec<Home>>,
        members: Mutex<Vec<(Uuid, Uuid, Role)>>,
    }

    /// The invitation half of `HomeRepo` is exercised in `homes.rs` and against the database; these tests are about
    /// device state, so the invite methods answer as an empty house rather than pretending.
    impl FakeHomes {
        fn no_invites(&self) -> Result<Vec<crate::homes::HomeInvite>, AppError> {
            Ok(vec![])
        }
    }

    impl HomeRepo for FakeHomes {
        async fn create_home(&self, home: &Home) -> Result<(), AppError> {
            self.homes.lock().unwrap().push(home.clone());
            Ok(())
        }
        async fn list_for_user(&self, _: Uuid) -> Result<Vec<Home>, AppError> {
            Ok(vec![])
        }
        async fn get(&self, home_id: Uuid, user_id: Uuid) -> Result<Option<Home>, AppError> {
            let Some(role) = self
                .members
                .lock()
                .unwrap()
                .iter()
                .find(|m| m.0 == home_id && m.1 == user_id)
                .map(|m| m.2)
            else {
                return Ok(None);
            };
            Ok(self
                .homes
                .lock()
                .unwrap()
                .iter()
                .find(|h| h.id == home_id)
                .map(|h| Home { role, ..h.clone() }))
        }
        async fn rename(&self, _: Uuid, _: &str) -> Result<(), AppError> {
            Ok(())
        }

        async fn set_timezone(&self, _: Uuid, _: &str) -> Result<(), AppError> {
            Ok(())
        }

        /// The fake knows one zone, which is the shape the tests care about: a real name passes, a made-up
        /// one does not.
        async fn timezone_exists(&self, timezone: &str) -> Result<bool, AppError> {
            Ok(timezone == "Europe/Berlin" || timezone == "UTC")
        }
        async fn delete(&self, _: Uuid) -> Result<(), AppError> {
            Ok(())
        }
        async fn members(&self, _: Uuid) -> Result<Vec<Member>, AppError> {
            Ok(vec![])
        }
        async fn add_member(&self, _: Uuid, _: Uuid, _: Role, _: bool) -> Result<(), AppError> {
            Ok(())
        }
        async fn find_member_by_email(&self, _: Uuid, _: &str) -> Result<Option<Uuid>, AppError> {
            Ok(None)
        }
        async fn set_member_role(&self, _: Uuid, _: Uuid, _: Role) -> Result<(), AppError> {
            Ok(())
        }
        async fn remove_member(&self, _: Uuid, _: Uuid) -> Result<(), AppError> {
            Ok(())
        }
        async fn rooms(&self, _: Uuid) -> Result<Vec<Room>, AppError> {
            Ok(vec![])
        }
        async fn create_room(&self, _: &Room) -> Result<(), AppError> {
            Ok(())
        }
        async fn rename_room(&self, _: Uuid, _: &str) -> Result<(), AppError> {
            Ok(())
        }
        async fn set_location(
            &self,
            _: Uuid,
            lat: Option<f64>,
            lon: Option<f64>,
        ) -> Result<(), AppError> {
            for home in self.homes.lock().unwrap().iter_mut() {
                home.geo_lat = lat;
                home.geo_lon = lon;
            }
            Ok(())
        }

        async fn set_presence(&self, _: Uuid, _: Uuid, _: bool) -> Result<(), AppError> {
            Ok(())
        }

        async fn delete_room(&self, _: Uuid) -> Result<(), AppError> {
            Ok(())
        }
        async fn create_invite(&self, _: &crate::homes::HomeInvite) -> Result<(), AppError> {
            Ok(())
        }
        async fn invites(&self, _: Uuid) -> Result<Vec<crate::homes::HomeInvite>, AppError> {
            self.no_invites()
        }
        async fn invites_for_email(
            &self,
            _: &str,
        ) -> Result<Vec<crate::homes::HomeInvite>, AppError> {
            self.no_invites()
        }
        async fn invite_by_hash(
            &self,
            _: &str,
        ) -> Result<Option<crate::homes::HomeInvite>, AppError> {
            Ok(None)
        }
        async fn accept_invite(&self, _: Uuid, _: Uuid) -> Result<(), AppError> {
            Ok(())
        }
        async fn revoke_invite(&self, _: Uuid, _: Uuid) -> Result<(), AppError> {
            Ok(())
        }

        async fn room_belongs_to(&self, _: Uuid, _: Uuid) -> Result<bool, AppError> {
            Ok(true)
        }
    }

    #[derive(Default)]
    struct FakeDevices {
        devices: Mutex<Vec<Device>>,
        releases: Mutex<Vec<Release>>,
    }

    impl DeviceRepo for FakeDevices {
        async fn list_for_home(
            &self,
            home_id: Uuid,
            _user: Uuid,
            search: Option<&str>,
        ) -> Result<Vec<Device>, AppError> {
            // The fake honours the query the same way the SQL does — case-insensitive contains on name or
            // serial — so a service test cannot pass on a search the repository would have applied.
            let needle = search.map(str::to_lowercase);
            Ok(self
                .devices
                .lock()
                .unwrap()
                .iter()
                .filter(|d| d.home_id == Some(home_id))
                .filter(|d| match &needle {
                    None => true,
                    Some(q) => {
                        d.name.to_lowercase().contains(q) || d.serial.to_lowercase().contains(q)
                    }
                })
                .cloned()
                .collect())
        }
        /// The fake keeps no timestamps, so "silent" is "not online": the same shape, none of the clock.
        async fn home_presence(&self, home_id: Uuid, _user: Uuid) -> Result<Presence, AppError> {
            let devices = self.devices.lock().unwrap();
            let mine: Vec<&Device> = devices
                .iter()
                .filter(|d| d.home_id == Some(home_id))
                .collect();
            let online = mine.iter().filter(|d| d.online).count() as i64;
            Ok(Presence {
                total: mine.len() as i64,
                online,
                silent: mine.len() as i64 - online,
            })
        }

        async fn get(&self, id: Uuid, _user: Uuid) -> Result<Option<Device>, AppError> {
            Ok(self
                .devices
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.id == id)
                .cloned())
        }
        async fn home_of(&self, id: Uuid, _user: Uuid) -> Result<Option<Uuid>, AppError> {
            Ok(self
                .devices
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.id == id)
                .and_then(|d| d.home_id))
        }
        async fn rename(&self, id: Uuid, _user: Uuid, name: &str) -> Result<(), AppError> {
            if let Some(d) = self.devices.lock().unwrap().iter_mut().find(|d| d.id == id) {
                d.name = name.to_string();
            }
            Ok(())
        }
        async fn assign_room(
            &self,
            id: Uuid,
            _user: Uuid,
            room: Option<Uuid>,
        ) -> Result<(), AppError> {
            if let Some(d) = self.devices.lock().unwrap().iter_mut().find(|d| d.id == id) {
                d.room_id = room;
            }
            Ok(())
        }
        async fn unclaim(&self, id: Uuid, _user: Uuid) -> Result<(), AppError> {
            if let Some(d) = self.devices.lock().unwrap().iter_mut().find(|d| d.id == id) {
                d.home_id = None;
            }
            Ok(())
        }

        async fn record_release(&self, release: &Release) -> Result<(), AppError> {
            self.releases.lock().unwrap().push(release.clone());
            Ok(())
        }

        async fn latest_release(&self, id: Uuid) -> Result<Option<Release>, AppError> {
            Ok(self
                .releases
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.device_id == id)
                .max_by_key(|r| r.released_at)
                .cloned())
        }

        async fn restore(
            &self,
            id: Uuid,
            home_id: Uuid,
            _room_id: Option<Uuid>,
            _user: Uuid,
        ) -> Result<(), AppError> {
            if let Some(d) = self.devices.lock().unwrap().iter_mut().find(|d| d.id == id) {
                d.home_id = Some(home_id);
            }
            for release in self
                .releases
                .lock()
                .unwrap()
                .iter_mut()
                .filter(|r| r.device_id == id)
            {
                if release.restored_at.is_none() {
                    release.restored_at = Some(OffsetDateTime::now_utc());
                }
            }
            Ok(())
        }
    }

    struct Fixture {
        svc: DeviceService<FakeDevices, FakeHomes>,
        owner: Uuid,
        member: Uuid,
        home: Uuid,
        device: Uuid,
    }

    fn fixture() -> Fixture {
        let homes = Arc::new(FakeHomes::default());
        let devices = Arc::new(FakeDevices::default());
        let owner = Uuid::new_v4();
        let member = Uuid::new_v4();
        let home = Uuid::new_v4();
        let device = Uuid::new_v4();

        homes.homes.lock().unwrap().push(Home {
            id: home,
            owner_id: owner,
            name: "Flat".into(),
            timezone: "UTC".into(),
            role: Role::Owner,
            member_count: 2,
            device_count: 1,
            geo_lat: None,
            geo_lon: None,
            created_at: kernel::now(),
        });
        homes
            .members
            .lock()
            .unwrap()
            .extend([(home, owner, Role::Owner), (home, member, Role::Member)]);
        devices.devices.lock().unwrap().push(Device {
            serial: "TEST-0001".into(),
            id: device,
            vendor_id: Uuid::new_v4(),
            home_id: Some(home),
            room_id: None,
            room_name: None,
            name: "Lamp".into(),
            product_key: "hv_acme_bulb1".into(),
            category: "light".into(),
            online: true,
            fw_version: Some("1.0.0".into()),
            last_seen_at: None,
            report_interval_secs: Some(600),
            points: vec![],
        });

        let home_svc = Arc::new(HomeService::new(homes));
        Fixture {
            svc: DeviceService::new(devices, home_svc),
            owner,
            member,
            home,
            device,
        }
    }

    #[tokio::test]
    async fn a_member_sees_the_home_devices() {
        let f = fixture();
        assert_eq!(f.svc.list(f.home, f.member, None).await.unwrap().len(), 1);
    }

    /// The reading's own clock has to reach the client, or a screen can only ask whether the *device* is
    /// reachable — and an online sensor with an eleven-hour-old temperature then has to be drawn as current.
    #[test]
    fn a_point_carries_when_its_reading_was_taken() {
        let at = time::OffsetDateTime::from_unix_timestamp(1_763_400_000).unwrap();
        let point = Point {
            id: Uuid::now_v7(),
            code: "temperature.value".into(),
            dp: "1".into(),
            capability: "temperature".into(),
            slot: "value".into(),
            kind: "float".into(),
            writable: false,
            unit: Some("°C".into()),
            min: None,
            max: None,
            history: Some("interval:300".into()),
            value: Some(PointValue::Num(21.5)),
            updated_at: Some(at),
        };
        let json = serde_json::to_value(&point).unwrap();
        assert_eq!(
            json["updated_at"], "2025-11-17T17:20:00Z",
            "RFC 3339, so every client parses it without guessing"
        );
        // A point that has never been written says so rather than inventing a time.
        let cold = Point {
            updated_at: None,
            ..point
        };
        assert!(serde_json::to_value(&cold).unwrap()["updated_at"].is_null());
    }

    #[tokio::test]
    async fn a_stranger_cannot_reach_a_device_by_id() {
        let f = fixture();
        let err = f.svc.get(f.device, Uuid::new_v4()).await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::NotFound);
    }

    #[tokio::test]
    async fn a_member_cannot_rename_but_an_owner_can() {
        let f = fixture();
        let err = f
            .svc
            .rename(f.device, f.member, "Hallway")
            .await
            .unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::Forbidden);

        let d = f.svc.rename(f.device, f.owner, "Hallway").await.unwrap();
        assert_eq!(d.name, "Hallway");
    }

    #[tokio::test]
    async fn unclaim_detaches_the_device_from_the_home() {
        let f = fixture();
        f.svc.unclaim(f.device, f.owner).await.unwrap();
        assert!(f.svc.list(f.home, f.owner, None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_empty_name_is_refused() {
        let f = fixture();
        let err = f.svc.rename(f.device, f.owner, "   ").await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);
    }
}
