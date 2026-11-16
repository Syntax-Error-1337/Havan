//! Vendor teams: invitations, roles, and the rules about who may change them.
//!
//! The rules are here, not in the handlers, so they are testable without a database: a viewer
//! cannot invite; an admin can invite but cannot touch another admin; nobody can demote or remove
//! the last owner, because a vendor with no owner is an account nobody can administer.

use std::future::Future;

use kernel::AppError;
use serde::Serialize;
use uuid::Uuid;

/// Seven days: long enough for someone to accept after a holiday, short enough that a leaked link
/// is not a permanent door.
pub const INVITE_TTL_HOURS: i64 = 168;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Owner,
    Admin,
    Developer,
    Viewer,
}

impl Role {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "owner" => Some(Role::Owner),
            "admin" => Some(Role::Admin),
            "developer" => Some(Role::Developer),
            "viewer" => Some(Role::Viewer),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::Developer => "developer",
            Role::Viewer => "viewer",
        }
    }

    /// `owner` and `admin` manage people; the other two do not, because "who is on my team" is not
    /// a developer's question to answer.
    pub fn can_manage_team(self) -> bool {
        matches!(self, Role::Owner | Role::Admin)
    }
}

/// May `actor` invite someone as `role`?
///
/// Two independent limits: the actor's own rank, and the rank being handed out. An admin can add a
/// developer; an admin cannot add another admin, because that is how a team ends up with two people
/// who can remove each other.
pub fn may_invite(actor: Role, role: Role) -> Result<(), AppError> {
    if !actor.can_manage_team() {
        return Err(AppError::forbidden(
            "only an owner or an admin can invite people",
        ));
    }
    match (actor, role) {
        (Role::Owner, Role::Admin | Role::Developer | Role::Viewer) => Ok(()),
        (Role::Admin, Role::Developer | Role::Viewer) => Ok(()),
        (Role::Admin, Role::Admin) => Err(AppError::forbidden(
            "an admin cannot invite another admin; ask the owner",
        )),
        _ => Err(AppError::validation("that role cannot be invited")),
    }
}

/// May `actor` remove a member whose role is `target`, leaving `owners_left` owners behind?
pub fn may_remove(actor: Role, target: Role, owners_left: i64) -> Result<(), AppError> {
    if !actor.can_manage_team() {
        return Err(AppError::forbidden(
            "only an owner or an admin can remove people",
        ));
    }
    if target == Role::Owner {
        if owners_left <= 1 {
            return Err(AppError::code(
                kernel::ErrorCode::Conflict,
                "this is the last owner; transfer ownership before removing them",
            ));
        }
        if actor != Role::Owner {
            return Err(AppError::forbidden("only an owner can remove an owner"));
        }
    }
    if actor == Role::Admin && target == Role::Admin {
        return Err(AppError::forbidden("an admin cannot remove another admin"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamMember {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub last_login_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PendingInvite {
    pub id: Uuid,
    pub email: String,
    pub role: String,
    pub expires_at: time::OffsetDateTime,
    pub expired: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Team {
    pub members: Vec<TeamMember>,
    pub invites: Vec<PendingInvite>,
}

pub trait VendorTeamRepo {
    /// Create an invitation and return its plaintext token — the only time it exists in the clear.
    fn invite(
        &self,
        vendor_id: Uuid,
        invited_by: Uuid,
        email: &str,
        role: Role,
    ) -> impl Future<Output = Result<String, AppError>> + Send;

    /// Accept an invitation: create the user with the invited role. Runs without a vendor context,
    /// because the person accepting has no session yet and the token is the credential.
    fn accept(
        &self,
        token: &str,
        display_name: &str,
        password: &str,
    ) -> impl Future<Output = Result<(Uuid, Uuid), AppError>> + Send;

    fn team(&self, vendor_id: Uuid) -> impl Future<Output = Result<Team, AppError>> + Send;

    fn role_of(
        &self,
        vendor_id: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Role, AppError>> + Send;

    fn remove(
        &self,
        vendor_id: Uuid,
        actor: Uuid,
        user_id: Uuid,
    ) -> impl Future<Output = Result<(), AppError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_can_invite_anything_but_owners() {
        for role in [Role::Admin, Role::Developer, Role::Viewer] {
            assert!(may_invite(Role::Owner, role).is_ok());
        }
        assert!(may_invite(Role::Owner, Role::Owner).is_err());
    }

    #[test]
    fn admins_cannot_create_peers() {
        assert!(may_invite(Role::Admin, Role::Admin).is_err());
        assert!(may_invite(Role::Admin, Role::Developer).is_ok());
        assert!(may_invite(Role::Admin, Role::Viewer).is_ok());
    }

    #[test]
    fn developers_and_viewers_cannot_invite_at_all() {
        for role in [Role::Developer, Role::Viewer] {
            assert!(may_invite(role, Role::Viewer).is_err());
        }
    }

    #[test]
    fn the_last_owner_cannot_be_removed_by_anyone() {
        assert!(may_remove(Role::Owner, Role::Owner, 1).is_err());
        assert!(may_remove(Role::Owner, Role::Owner, 2).is_ok());
    }

    #[test]
    fn admins_cannot_remove_admins_or_owners() {
        assert!(may_remove(Role::Admin, Role::Admin, 2).is_err());
        assert!(may_remove(Role::Admin, Role::Owner, 2).is_err());
        assert!(may_remove(Role::Admin, Role::Developer, 1).is_ok());
    }

    #[test]
    fn roles_round_trip_and_unknown_is_refused() {
        for role in [Role::Owner, Role::Admin, Role::Developer, Role::Viewer] {
            assert_eq!(Role::parse(role.as_str()), Some(role));
        }
        assert_eq!(Role::parse("root"), None);
    }
}
