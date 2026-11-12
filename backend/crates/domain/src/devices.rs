//! Devices as the app sees them: a name, a room, and the points a customer can
//! touch. Vendor-facing fleet operations live in the vendor slice.
//!
//! Authorisation is inherited from [`crate::homes`]: a device belongs to a home,
//! and only that home's members may see it — a device id is not a capability.

use std::sync::Arc;

use kernel::AppError;
use serde::{Deserialize, Serialize};
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
#[derive(Debug, Clone)]
pub struct Publishable {
    pub vendor_id: Uuid,
    pub home_id: Uuid,
    pub dp: String,
    pub value: PointValue,
}

#[derive(Debug, Clone)]
pub struct CommandOutcome {
    pub command: Command,
    /// `None` when an idempotent replay returned a command already in flight.
    pub publish: Option<Publishable>,
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
    pub last_seen_at: Option<time::OffsetDateTime>,
    pub points: Vec<Point>,
}

/// Every method takes the acting user: the repository runs inside a
/// transaction that carries `haven.user_id`, and the database's policy decides
/// what that user may see. A method without a user cannot be written.
pub trait DeviceRepo {
    fn list_for_home(
        &self,
        home_id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Vec<Device>, AppError>> + Send;

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
}

/// Commands live next to devices because they are authorised the same way:
/// by the membership of the home the device belongs to.
pub trait CommandRepo {
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

    pub async fn list(&self, home_id: Uuid, user_id: Uuid) -> Result<Vec<Device>, AppError> {
        self.homes.require(home_id, user_id).await?;
        self.devices.list_for_home(home_id, user_id).await
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
        let home = self
            .homes
            .require(self.device_home(device_id, user_id).await?, user_id)
            .await?;
        if home.role < Role::Admin {
            return Err(AppError::forbidden("only an admin can remove a device"));
        }
        self.devices.unclaim(device_id, user_id).await
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
            dp: point.dp.clone(),
            value,
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
        async fn room_belongs_to(&self, _: Uuid, _: Uuid) -> Result<bool, AppError> {
            Ok(true)
        }
    }

    #[derive(Default)]
    struct FakeDevices {
        devices: Mutex<Vec<Device>>,
    }

    impl DeviceRepo for FakeDevices {
        async fn list_for_home(&self, home_id: Uuid, _user: Uuid) -> Result<Vec<Device>, AppError> {
            Ok(self
                .devices
                .lock()
                .unwrap()
                .iter()
                .filter(|d| d.home_id == Some(home_id))
                .cloned()
                .collect())
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
        assert_eq!(f.svc.list(f.home, f.member).await.unwrap().len(), 1);
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
        assert!(f.svc.list(f.home, f.owner).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_empty_name_is_refused() {
        let f = fixture();
        let err = f.svc.rename(f.device, f.owner, "   ").await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);
    }
}
