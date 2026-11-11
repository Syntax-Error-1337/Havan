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

    fn delete(&self, home_id: Uuid) -> impl Future<Output = Result<(), AppError>> + Send;

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

    fn delete_room(&self, room_id: Uuid) -> impl Future<Output = Result<(), AppError>> + Send;

    fn room_belongs_to(
        &self,
        room_id: Uuid,
        home_id: Uuid,
    ) -> impl Future<Output = Result<bool, AppError>> + Send;
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
        if !valid_timezone(timezone) {
            return Err(AppError::validation("unknown timezone"));
        }

        let home = Home {
            id: kernel::new_id(),
            owner_id: user_id,
            name: name.to_string(),
            timezone: timezone.to_string(),
            role: Role::Owner,
            member_count: 1,
            device_count: 0,
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
    pub async fn invite(
        &self,
        home_id: Uuid,
        actor: Uuid,
        email: &str,
        role: Role,
    ) -> Result<(), AppError> {
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
        let target = self
            .repo
            .find_member_by_email(home_id, &email)
            .await?
            .ok_or_else(|| AppError::not_found("account with that email"))?;

        self.repo
            .add_member(home_id, target, role, role.can_manage())
            .await
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
    use kernel::ErrorCode;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeRepo {
        homes: Mutex<Vec<Home>>,
        members: Mutex<Vec<(Uuid, Uuid, Role)>>,
        rooms: Mutex<Vec<Room>>,
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
        async fn delete_room(&self, room_id: Uuid) -> Result<(), AppError> {
            self.rooms.lock().unwrap().retain(|r| r.id != room_id);
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
