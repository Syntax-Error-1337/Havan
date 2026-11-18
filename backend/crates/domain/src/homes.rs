//! Homes, who belongs to them, and their rooms.
//!
//! This slice owns the one rule the rest of the platform depends on: access to a
//! home is membership, and a role decides what membership allows. Devices,
//! scenes and telemetry all defer to [`Role`].

use std::sync::Arc;

use kernel::AppError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Guest,
    Member,
    Admin,
    Owner,
}

impl Role {
    pub fn parse(s: &str) -> Option<Role> {
        Some(match s {
            "guest" => Role::Guest,
            "member" => Role::Member,
            "admin" => Role::Admin,
            "owner" => Role::Owner,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Guest => "guest",
            Role::Member => "member",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }

    /// Control a device, run a scene.
    pub fn can_control(self) -> bool {
        self >= Role::Guest
    }

    /// Rename devices, edit scenes, invite members.
    pub fn can_manage(self) -> bool {
        self >= Role::Admin
    }

    /// Delete the home, transfer ownership, add hardware.
    pub fn can_own(self) -> bool {
        self >= Role::Owner
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Home {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub timezone: String,
    pub role: Role,
    pub member_count: i64,
    pub device_count: i64,
    /// Coordinates for sunrise/sunset. `None` until somebody sets them, and solar
    /// automations refuse to be created without them.
    pub geo_lat: Option<f64>,
    pub geo_lon: Option<f64>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub user_id: Uuid,
    pub display_name: String,
    pub email: Option<String>,
    pub role: Role,
    pub can_pair: bool,
}

/// An invitation to a home, for somebody who may not have an account yet.
///
/// `token_hash` never leaves the service: the plaintext token is returned to the inviter once, at creation, and the row
/// keeps only its SHA-256. `HomeInvite` is serialised to the app without the hash field at all (see `#[serde(skip)]`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeInvite {
    pub id: Uuid,
    pub home_id: Uuid,
    pub email: String,
    pub role: Role,
    #[serde(skip)]
    pub token_hash: String,
    pub invited_by: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub accepted_at: Option<time::OffsetDateTime>,
}

impl HomeInvite {
    /// Whether this invitation can still be accepted: not accepted, not expired.
    pub fn is_live(&self, now: time::OffsetDateTime) -> bool {
        self.accepted_at.is_none() && self.expires_at > now
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: Uuid,
    pub home_id: Uuid,
    pub name: String,
    pub icon: Option<String>,
    pub sort: i32,
}

/// Storage the slice needs. Implemented by `infra`; faked in tests.
pub trait HomeRepo {
    fn create_home(&self, home: &Home) -> impl Future<Output = Result<(), AppError>> + Send;

    fn list_for_user(
        &self,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Vec<Home>, AppError>> + Send;

    fn get(
        &self,
        home_id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Option<Home>, AppError>> + Send;

    fn rename(
        &self,
        home_id: Uuid,
        name: &str,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn set_timezone(
        &self,
        home_id: Uuid,
        timezone: &str,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Is this a timezone the platform can actually schedule in?
    ///
    /// `valid_timezone` only checks the shape of the string ("Mars/Olympus" passes), and its own comment
    /// said to upgrade to a real lookup once schedules existed. They exist: an automation at 07:00 is
    /// evaluated in the home's zone, so a zone the database does not know is a schedule that never fires
    /// or fires at the wrong hour, silently. Postgres already ships the authority (`pg_timezone_names`).
    fn timezone_exists(
        &self,
        timezone: &str,
    ) -> impl Future<Output = Result<bool, AppError>> + Send;

    fn delete(&self, home_id: Uuid) -> impl Future<Output = Result<(), AppError>> + Send;

    fn set_location(
        &self,
        home_id: Uuid,
        lat: Option<f64>,
        lon: Option<f64>,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Mark one member present or away.
    fn set_presence(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        present: bool,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn members(&self, home_id: Uuid) -> impl Future<Output = Result<Vec<Member>, AppError>> + Send;

    fn add_member(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        role: Role,
        can_pair: bool,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn find_member_by_email(
        &self,
        home_id: Uuid,
        email: &str,
    ) -> impl Future<Output = Result<Option<Uuid>, AppError>> + Send;

    fn set_member_role(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        role: Role,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn remove_member(
        &self,
        home_id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn rooms(&self, home_id: Uuid) -> impl Future<Output = Result<Vec<Room>, AppError>> + Send;

    fn create_room(&self, room: &Room) -> impl Future<Output = Result<(), AppError>> + Send;

    fn rename_room(
        &self,
        room_id: Uuid,
        name: &str,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn delete_room(&self, room_id: Uuid) -> impl Future<Output = Result<(), AppError>> + Send;

    fn room_belongs_to(
        &self,
        room_id: Uuid,
        home_id: Uuid,
    ) -> impl Future<Output = Result<bool, AppError>> + Send;

    // ---- invitations --------------------------------------------------------------------------------------------

    /// Create (or replace) the live invitation for one address in one home. Returns the row's id.
    fn create_invite(
        &self,
        invite: &HomeInvite,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn invites(
        &self,
        home_id: Uuid,
    ) -> impl Future<Output = Result<Vec<HomeInvite>, AppError>> + Send;

    /// Pending invitations for one address, across every home. Used when somebody signs up: an invitation that
    /// arrived before the account did should not need a second email to be useful.
    fn invites_for_email(
        &self,
        email: &str,
    ) -> impl Future<Output = Result<Vec<HomeInvite>, AppError>> + Send;

    /// The live invitation behind a token hash, if it is unaccepted and unexpired.
    fn invite_by_hash(
        &self,
        token_hash: &str,
    ) -> impl Future<Output = Result<Option<HomeInvite>, AppError>> + Send;

    fn accept_invite(
        &self,
        id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn revoke_invite(
        &self,
        home_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = Result<(), AppError>> + Send;
}

/// How long an invitation lives. A week is long enough to survive a weekend and short enough that a link forwarded by
/// mistake stops working.
pub const INVITE_DAYS: i64 = 7;

/// What `invite` did: added an existing account, or created an invitation for an address that has none.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum InviteOutcome {
    Added,
    Invited {
        /// Shown once. The platform has no mail transport, so the person who created it sends the link.
        token: String,
        #[serde(with = "time::serde::rfc3339")]
        expires_at: time::OffsetDateTime,
    },
}

fn token() -> String {
    use base64::Engine;
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_token(secret: &str) -> String {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(secret.as_bytes()))
}

pub struct HomeService<R> {
    repo: Arc<R>,
}

impl<R: HomeRepo + Send + Sync> HomeService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    pub async fn create(
        &self,
        user_id: Uuid,
        name: &str,
        timezone: &str,
    ) -> Result<Home, AppError> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err(AppError::validation("home name must be 1–60 characters"));
        }
        // The shape first (cheap, and it keeps the error message specific), then the real list — a home
        // whose zone the scheduler cannot resolve has schedules that never fire.
        if !valid_timezone(timezone) || !self.repo.timezone_exists(timezone).await? {
            return Err(AppError::validation(
                "that is not a timezone this platform knows; use an IANA name like Europe/Berlin",
            ));
        }

        let home = Home {
            id: kernel::new_id(),
            owner_id: user_id,
            name: name.to_string(),
            timezone: timezone.to_string(),
            role: Role::Owner,
            member_count: 1,
            device_count: 0,
            geo_lat: None,
            geo_lon: None,
            created_at: kernel::now(),
        };
        self.repo.create_home(&home).await?;
        Ok(home)
    }

    pub async fn list(&self, user_id: Uuid) -> Result<Vec<Home>, AppError> {
        self.repo.list_for_user(user_id).await
    }

    /// Membership check. Every other operation calls this first — there is no
    /// path into a home that does not pass through here.
    pub async fn require(&self, home_id: Uuid, user_id: Uuid) -> Result<Home, AppError> {
        self.repo
            .get(home_id, user_id)
            .await?
            .ok_or_else(|| AppError::not_found("home"))
    }

    /// Change the home's timezone.
    ///
    /// The home's zone is what every schedule is read against, so this is a manager's decision, and it is
    /// checked against the real list rather than the shape of the string.
    pub async fn set_timezone(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        timezone: &str,
    ) -> Result<Home, AppError> {
        let home = self.require(home_id, user_id).await?;
        if !home.role.can_manage() {
            return Err(AppError::forbidden(
                "only an admin can change a home's timezone",
            ));
        }
        let timezone = timezone.trim();
        if !valid_timezone(timezone) || !self.repo.timezone_exists(timezone).await? {
            return Err(AppError::validation(
                "that is not a timezone this platform knows; use an IANA name like Europe/Berlin",
            ));
        }
        self.repo.set_timezone(home_id, timezone).await?;
        Ok(Home {
            timezone: timezone.to_string(),
            ..home
        })
    }

    pub async fn rename(&self, home_id: Uuid, user_id: Uuid, name: &str) -> Result<Home, AppError> {
        let home = self.require(home_id, user_id).await?;
        if !home.role.can_manage() {
            return Err(AppError::forbidden("only an admin can rename a home"));
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 60 {
            return Err(AppError::validation("home name must be 1–60 characters"));
        }
        self.repo.rename(home_id, name).await?;
        Ok(Home {
            name: name.to_string(),
            ..home
        })
    }

    /// Set or clear the home's coordinates. Latitude and longitude move together:
    /// half a coordinate is a home somewhere on a line, which is not a place.
    pub async fn set_location(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        lat: Option<f64>,
        lon: Option<f64>,
    ) -> Result<Home, AppError> {
        let home = self.require(home_id, user_id).await?;
        if !home.role.can_manage() {
            return Err(AppError::forbidden("only an admin can set the location"));
        }
        match (lat, lon) {
            (None, None) => {}
            (Some(lat), Some(lon)) => {
                if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                    return Err(AppError::validation(
                        "latitude must be -90..90 and longitude -180..180",
                    ));
                }
            }
            _ => {
                return Err(AppError::validation(
                    "latitude and longitude must be set together",
                ));
            }
        }
        self.repo.set_location(home_id, lat, lon).await?;
        Ok(Home {
            geo_lat: lat,
            geo_lon: lon,
            ..home
        })
    }

    /// Say whether you are at home. Each member answers for themselves; a home is
    /// occupied when at least one does.
    pub async fn set_presence(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        present: bool,
    ) -> Result<(), AppError> {
        // Membership is the only requirement: a guest may say they left.
        self.require(home_id, user_id).await?;
        self.repo.set_presence(home_id, user_id, present).await
    }

    pub async fn delete(&self, home_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
        let home = self.require(home_id, user_id).await?;
        if home.owner_id != user_id {
            return Err(AppError::forbidden("only the owner can delete a home"));
        }
        self.repo.delete(home_id).await
    }

    pub async fn members(&self, home_id: Uuid, user_id: Uuid) -> Result<Vec<Member>, AppError> {
        self.require(home_id, user_id).await?;
        self.repo.members(home_id).await
    }

    /// Invite by email. The invitee must already have an account in v1 —
    /// pending invitations are Phase 6, and pretending otherwise silently
    /// creates orphan rows.
    /// Add somebody to a home, or invite them if they have no account yet.
    ///
    /// The old behaviour was the first half only: `POST /homes/{id}/members` required the address to belong to an
    /// existing account and returned 404 otherwise — so the person you actually want in your family home, the one who has
    /// never installed the app, could not be added at all. Now an unknown address gets an invitation with a token, shown
    /// once because there is still no mail transport.
    pub async fn invite(
        &self,
        home_id: Uuid,
        actor: Uuid,
        email: &str,
        role: Role,
    ) -> Result<InviteOutcome, AppError> {
        let home = self.require(home_id, actor).await?;
        if !home.role.can_manage() {
            return Err(AppError::forbidden("only an admin can invite members"));
        }
        if role == Role::Owner {
            return Err(AppError::validation(
                "use ownership transfer to add an owner",
            ));
        }
        if role > home.role {
            return Err(AppError::forbidden(
                "you cannot grant a role above your own",
            ));
        }

        let email = crate::identity::normalize_email(email);
        if let Some(target) = self.repo.find_member_by_email(home_id, &email).await? {
            self.repo
                .add_member(home_id, target, role, role.can_manage())
                .await?;
            return Ok(InviteOutcome::Added);
        }

        // No account yet: an invitation instead of a 404. The token is the credential, so it is generated here, hashed
        // for storage and returned once — the same shape as a vendor invitation, for the same reason.
        let token = crate::homes::token();
        let now = kernel::now();
        let invite = HomeInvite {
            id: kernel::new_id(),
            home_id,
            email,
            role,
            token_hash: crate::homes::hash_token(&token),
            invited_by: actor,
            created_at: now,
            expires_at: now + time::Duration::days(INVITE_DAYS),
            accepted_at: None,
        };

        self.repo.create_invite(&invite).await?;
        Ok(InviteOutcome::Invited {
            token,
            expires_at: invite.expires_at,
        })
    }

    /// Pending invitations for a home. Admin-only: a guest does not need to know who is being asked to join.
    pub async fn invites(&self, home_id: Uuid, actor: Uuid) -> Result<Vec<HomeInvite>, AppError> {
        let home = self.require(home_id, actor).await?;
        if !home.role.can_manage() {
            return Err(AppError::forbidden("only an admin can see invitations"));
        }
        self.repo.invites(home_id).await
    }

    /// Accept an invitation by its token.
    ///
    /// The address has to match the signed-in account: the token is a bearer credential, and without this check a
    /// forwarded link would let the wrong person into somebody's home.
    pub async fn accept_invite(
        &self,
        actor: Uuid,
        actor_email: &str,
        token: &str,
    ) -> Result<Uuid, AppError> {
        let hash = crate::homes::hash_token(token);
        let invite = self
            .repo
            .invite_by_hash(&hash)
            .await?
            .ok_or_else(|| AppError::not_found("invitation"))?;

        if !invite.is_live(kernel::now()) {
            return Err(AppError::validation(
                "that invitation has expired or was already used",
            ));
        }
        if crate::identity::normalize_email(actor_email) != invite.email {
            return Err(AppError::forbidden(
                "this invitation was sent to a different address",
            ));
        }

        self.repo
            .add_member(invite.home_id, actor, invite.role, invite.role.can_manage())
            .await?;
        self.repo.accept_invite(invite.id, actor).await?;
        Ok(invite.home_id)
    }

    pub async fn revoke_invite(
        &self,
        home_id: Uuid,
        actor: Uuid,
        id: Uuid,
    ) -> Result<(), AppError> {
        let home = self.require(home_id, actor).await?;
        if !home.role.can_manage() {
            return Err(AppError::forbidden(
                "only an admin can revoke an invitation",
            ));
        }
        self.repo.revoke_invite(home_id, id).await
    }

    pub async fn set_role(
        &self,
        home_id: Uuid,
        actor: Uuid,
        target: Uuid,
        role: Role,
    ) -> Result<(), AppError> {
        let home = self.require(home_id, actor).await?;
        if !home.role.can_manage() {
            return Err(AppError::forbidden("only an admin can change roles"));
        }
        if role == Role::Owner || target == home.owner_id {
            return Err(AppError::validation(
                "ownership transfer is a separate, confirmed operation",
            ));
        }
        if role > home.role {
            return Err(AppError::forbidden(
                "you cannot grant a role above your own",
            ));
        }
        self.repo.set_member_role(home_id, target, role).await
    }

    pub async fn remove_member(
        &self,
        home_id: Uuid,
        actor: Uuid,
        target: Uuid,
    ) -> Result<(), AppError> {
        let home = self.require(home_id, actor).await?;

        // Leaving is always allowed; removing someone else needs admin.
        if target != actor && !home.role.can_manage() {
            return Err(AppError::forbidden("only an admin can remove a member"));
        }
        if target == home.owner_id {
            return Err(AppError::validation(
                "the owner cannot leave; transfer ownership first",
            ));
        }
        self.repo.remove_member(home_id, target).await
    }

    pub async fn rooms(&self, home_id: Uuid, user_id: Uuid) -> Result<Vec<Room>, AppError> {
        self.require(home_id, user_id).await?;
        self.repo.rooms(home_id).await
    }

    pub async fn create_room(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        name: &str,
        icon: Option<&str>,
    ) -> Result<Room, AppError> {
        let home = self.require(home_id, user_id).await?;
        if !home.role.can_manage() {
            return Err(AppError::forbidden("only an admin can add a room"));
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 40 {
            return Err(AppError::validation("room name must be 1–40 characters"));
        }
        let room = Room {
            id: kernel::new_id(),
            home_id,
            name: name.to_string(),
            icon: icon.map(str::to_string),
            sort: 0,
        };
        self.repo.create_room(&room).await?;
        Ok(room)
    }

    pub async fn rename_room(
        &self,
        room_id: Uuid,
        home_id: Uuid,
        user_id: Uuid,
        name: &str,
    ) -> Result<(), AppError> {
        let home = self.require(home_id, user_id).await?;
        if !home.role.can_manage() {
            return Err(AppError::forbidden("only an admin can rename a room"));
        }
        if !self.repo.room_belongs_to(room_id, home_id).await? {
            return Err(AppError::not_found("room"));
        }
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 40 {
            return Err(AppError::validation("room name must be 1–40 characters"));
        }
        self.repo.rename_room(room_id, name).await
    }

    pub async fn delete_room(
        &self,
        room_id: Uuid,
        home_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        let home = self.require(home_id, user_id).await?;
        if !home.role.can_manage() {
            return Err(AppError::forbidden("only an admin can remove a room"));
        }
        if !self.repo.room_belongs_to(room_id, home_id).await? {
            return Err(AppError::not_found("room"));
        }
        self.repo.delete_room(room_id).await
    }
}

/// A syntactic check on the IANA name.
///
/// shortcut: shape only, not the tz database — "Mars/Olympus" passes. Upgrade to
/// a real lookup when schedules exist (Phase 6), because a wrong zone shifts
/// every automation silently.
fn valid_timezone(tz: &str) -> bool {
    if tz.is_empty() || tz.len() > 64 || tz == "UTC" {
        return !tz.is_empty() && tz.len() <= 64;
    }
    // `Area/Location`, optionally `Area/Location/Sub`.
    let parts: Vec<&str> = tz.split('/').collect();
    (2..=3).contains(&parts.len())
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '+')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_timezone_has_to_be_one_the_scheduler_can_use() {
        let svc = HomeService::new(Arc::new(FakeRepo::default()));
        let owner = Uuid::now_v7();
        let home = svc
            .create(owner, "Flat 3", "Asia/Taipei")
            .await
            .expect("create");

        // A real IANA name is accepted, and comes back on the home.
        let moved = svc
            .set_timezone(home.id, owner, "Europe/Berlin")
            .await
            .expect("a real zone");
        assert_eq!(moved.timezone, "Europe/Berlin");

        // A shape-valid invention is refused: `valid_timezone` alone would have taken it, and every
        // schedule in the home would then be read against a zone that does not exist.
        let err = svc
            .set_timezone(home.id, owner, "Mars/Olympus")
            .await
            .unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);

        // And a member cannot move everybody's schedules.
        let guest = Uuid::now_v7();
        svc.repo
            .add_member(home.id, guest, Role::Member, false)
            .await
            .expect("add member");
        let err = svc.set_timezone(home.id, guest, "UTC").await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::Forbidden);
    }
    use kernel::ErrorCode;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeRepo {
        homes: Mutex<Vec<Home>>,
        members: Mutex<Vec<(Uuid, Uuid, Role)>>,
        rooms: Mutex<Vec<Room>>,
        invites: Mutex<Vec<HomeInvite>>,
    }

    impl HomeRepo for FakeRepo {
        async fn create_home(&self, home: &Home) -> Result<(), AppError> {
            self.homes.lock().unwrap().push(home.clone());
            self.members
                .lock()
                .unwrap()
                .push((home.id, home.owner_id, Role::Owner));
            Ok(())
        }
        async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<Home>, AppError> {
            let members = self.members.lock().unwrap().clone();
            Ok(self
                .homes
                .lock()
                .unwrap()
                .iter()
                .filter(|h| members.iter().any(|m| m.0 == h.id && m.1 == user_id))
                .cloned()
                .collect())
        }
        async fn get(&self, home_id: Uuid, user_id: Uuid) -> Result<Option<Home>, AppError> {
            // Membership first: a non-member must not learn the home exists.
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
        async fn set_timezone(&self, home_id: Uuid, timezone: &str) -> Result<(), AppError> {
            if let Some(h) = self
                .homes
                .lock()
                .unwrap()
                .iter_mut()
                .find(|h| h.id == home_id)
            {
                h.timezone = timezone.to_string();
            }
            Ok(())
        }
        /// The fake knows the zones the tests use, and nothing else: a made-up name must fail here exactly
        /// as it does in Postgres.
        async fn timezone_exists(&self, timezone: &str) -> Result<bool, AppError> {
            Ok(matches!(
                timezone,
                "UTC" | "Europe/Berlin" | "Asia/Taipei" | "Asia/Kolkata" | "America/New_York"
            ))
        }
        async fn rename(&self, home_id: Uuid, name: &str) -> Result<(), AppError> {
            if let Some(h) = self
                .homes
                .lock()
                .unwrap()
                .iter_mut()
                .find(|h| h.id == home_id)
            {
                h.name = name.to_string();
            }
            Ok(())
        }
        async fn delete(&self, home_id: Uuid) -> Result<(), AppError> {
            self.homes.lock().unwrap().retain(|h| h.id != home_id);
            Ok(())
        }
        async fn members(&self, _home_id: Uuid) -> Result<Vec<Member>, AppError> {
            Ok(vec![])
        }
        async fn add_member(
            &self,
            home_id: Uuid,
            user_id: Uuid,
            role: Role,
            _can_pair: bool,
        ) -> Result<(), AppError> {
            self.members.lock().unwrap().push((home_id, user_id, role));
            Ok(())
        }
        async fn find_member_by_email(
            &self,
            _home_id: Uuid,
            email: &str,
        ) -> Result<Option<Uuid>, AppError> {
            Ok((email == "friend@example.com").then(Uuid::new_v4))
        }
        async fn set_member_role(
            &self,
            home_id: Uuid,
            user_id: Uuid,
            role: Role,
        ) -> Result<(), AppError> {
            if let Some(m) = self
                .members
                .lock()
                .unwrap()
                .iter_mut()
                .find(|m| m.0 == home_id && m.1 == user_id)
            {
                m.2 = role;
            }
            Ok(())
        }
        async fn remove_member(&self, home_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
            self.members
                .lock()
                .unwrap()
                .retain(|m| !(m.0 == home_id && m.1 == user_id));
            Ok(())
        }
        async fn rooms(&self, home_id: Uuid) -> Result<Vec<Room>, AppError> {
            Ok(self
                .rooms
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.home_id == home_id)
                .cloned()
                .collect())
        }
        async fn create_room(&self, room: &Room) -> Result<(), AppError> {
            self.rooms.lock().unwrap().push(room.clone());
            Ok(())
        }

        async fn rename_room(&self, room_id: Uuid, name: &str) -> Result<(), AppError> {
            // Stored, not ignored: a fake that accepts a rename without applying it
            // makes a broken rename look tested.
            if let Some(room) = self
                .rooms
                .lock()
                .unwrap()
                .iter_mut()
                .find(|r| r.id == room_id)
            {
                room.name = name.to_string();
            }
            Ok(())
        }
        async fn set_location(
            &self,
            _: Uuid,
            lat: Option<f64>,
            lon: Option<f64>,
        ) -> Result<(), AppError> {
            // Remember it: a fake that forgets coordinates makes a passing solar test
            // meaningless.
            let mut homes = self.homes.lock().unwrap();
            for home in homes.iter_mut() {
                home.geo_lat = lat;
                home.geo_lon = lon;
            }
            Ok(())
        }

        async fn set_presence(&self, _: Uuid, _: Uuid, _: bool) -> Result<(), AppError> {
            Ok(())
        }

        async fn delete_room(&self, room_id: Uuid) -> Result<(), AppError> {
            self.rooms.lock().unwrap().retain(|r| r.id != room_id);
            Ok(())
        }
        async fn create_invite(&self, invite: &HomeInvite) -> Result<(), AppError> {
            // Re-inviting replaces the live invitation for that address, like the real one.
            self.invites.lock().unwrap().retain(|i| {
                !(i.home_id == invite.home_id
                    && i.email.eq_ignore_ascii_case(&invite.email)
                    && i.accepted_at.is_none())
            });
            self.invites.lock().unwrap().push(invite.clone());
            Ok(())
        }
        async fn invites(&self, home_id: Uuid) -> Result<Vec<HomeInvite>, AppError> {
            Ok(self
                .invites
                .lock()
                .unwrap()
                .iter()
                .filter(|i| i.home_id == home_id && i.accepted_at.is_none())
                .cloned()
                .collect())
        }
        async fn invites_for_email(&self, email: &str) -> Result<Vec<HomeInvite>, AppError> {
            Ok(self
                .invites
                .lock()
                .unwrap()
                .iter()
                .filter(|i| {
                    i.email.eq_ignore_ascii_case(email)
                        && i.accepted_at.is_none()
                        && i.expires_at > kernel::now()
                })
                .cloned()
                .collect())
        }
        async fn invite_by_hash(&self, token_hash: &str) -> Result<Option<HomeInvite>, AppError> {
            Ok(self
                .invites
                .lock()
                .unwrap()
                .iter()
                .find(|i| i.token_hash == token_hash)
                .cloned())
        }
        async fn accept_invite(&self, id: Uuid, _: Uuid) -> Result<(), AppError> {
            let mut invites = self.invites.lock().unwrap();
            let Some(row) = invites.iter_mut().find(|i| i.id == id) else {
                return Err(AppError::not_found("invitation"));
            };
            if row.accepted_at.is_some() {
                return Err(AppError::validation("that invitation was already used"));
            }
            row.accepted_at = Some(kernel::now());
            Ok(())
        }
        async fn revoke_invite(&self, home_id: Uuid, id: Uuid) -> Result<(), AppError> {
            self.invites
                .lock()
                .unwrap()
                .retain(|i| !(i.id == id && i.home_id == home_id));
            Ok(())
        }

        async fn room_belongs_to(&self, room_id: Uuid, home_id: Uuid) -> Result<bool, AppError> {
            Ok(self
                .rooms
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.id == room_id && r.home_id == home_id))
        }
    }

    fn service() -> HomeService<FakeRepo> {
        HomeService::new(Arc::new(FakeRepo::default()))
    }

    #[tokio::test]
    async fn the_creator_is_the_owner_and_sees_the_home() {
        let svc = service();
        let user = Uuid::new_v4();
        let home = svc.create(user, "Flat", "Asia/Kolkata").await.unwrap();
        assert_eq!(home.role, Role::Owner);

        let listed = svc.list(user).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, home.id);
    }

    #[tokio::test]
    async fn a_stranger_gets_not_found_not_forbidden() {
        // 403 on a home you cannot see is an existence oracle.
        let svc = service();
        let owner = Uuid::new_v4();
        let home = svc.create(owner, "Flat", "UTC").await.unwrap();
        let err = svc.require(home.id, Uuid::new_v4()).await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::NotFound);
    }

    #[tokio::test]
    async fn only_the_owner_can_delete() {
        let svc = service();
        let owner = Uuid::new_v4();
        let other = Uuid::new_v4();
        let home = svc.create(owner, "Flat", "UTC").await.unwrap();

        let err = svc.delete(home.id, other).await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::NotFound);

        svc.delete(home.id, owner).await.unwrap();
        assert!(svc.list(owner).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_member_cannot_invite() {
        let svc = service();
        let owner = Uuid::new_v4();
        let member = Uuid::new_v4();
        let home = svc.create(owner, "Flat", "UTC").await.unwrap();
        svc.repo
            .add_member(home.id, member, Role::Member, false)
            .await
            .unwrap();

        let err = svc
            .invite(home.id, member, "friend@example.com", Role::Member)
            .await
            .unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::Forbidden);
    }

    #[tokio::test]
    async fn an_admin_cannot_grant_owner() {
        let svc = service();
        let owner = Uuid::new_v4();
        let admin = Uuid::new_v4();
        let home = svc.create(owner, "Flat", "UTC").await.unwrap();
        svc.repo
            .add_member(home.id, admin, Role::Admin, true)
            .await
            .unwrap();

        let err = svc
            .invite(home.id, admin, "friend@example.com", Role::Owner)
            .await
            .unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);
    }

    #[tokio::test]
    async fn timezones_are_checked() {
        assert!(valid_timezone("Asia/Kolkata"));
        assert!(valid_timezone("America/Argentina/Buenos_Aires"));
        assert!(valid_timezone("UTC"));
        assert!(!valid_timezone(""));
        assert!(!valid_timezone("kolkata")); // no area: rejected
        assert!(!valid_timezone("Asia/"));
    }
}
