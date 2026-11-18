//! Vendor team: invitations, members, and acceptance.
//!
//! Everything except `accept` is vendor-authenticated. `accept` is public on purpose: the person
//! following an invitation has no session — the token is the credential — and the row it creates is
//! scoped to the vendor that issued it, never to a vendor in the request.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use domain::vendor_team::{Role, Team, VendorTeamRepo, may_invite};
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::VendorAuth;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/vendor/team", get(team).post(invite))
        .route("/vendor/team/members/{id}", delete(remove))
        .route("/vendor/team/invites/{id}", delete(revoke_invite))
        .route("/vendor/invites/accept", post(accept))
}

async fn team(State(state): State<AppState>, vendor: VendorAuth) -> ApiResult<Json<Team>> {
    Ok(Json(
        state
            .vendor_team
            .team(vendor.vendor_id)
            .await
            .map_err(ApiError)?,
    ))
}

#[derive(Deserialize)]
struct InviteBody {
    email: String,
    role: String,
}

/// `POST /v1/vendor/team` — invite someone. The token comes back **once**: there is no mail
/// transport in the platform yet, so the operator sends the link themselves, and re-sending means
/// issuing a new invitation.
async fn invite(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Json(body): Json<InviteBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let actor_role: Role = state
        .vendor_team
        .role_of(vendor.vendor_id, vendor.user_id)
        .await
        .map_err(ApiError)?;

    let invited_role = Role::parse(&body.role)
        .ok_or_else(|| ApiError(kernel::AppError::validation("unknown role")))?;
    may_invite(actor_role, invited_role).map_err(ApiError)?;

    let token = state
        .vendor_team
        .invite(vendor.vendor_id, vendor.user_id, &body.email, invited_role)
        .await
        .map_err(ApiError)?;

    state
        .audit
        .record(
            domain::audit::AuditEntry::new(
                vendor.vendor_id,
                domain::audit::ActorKind::User,
                "team.invited",
                "vendor_user",
            )
            .by(vendor.user_id)
            .with_request(&vendor.request_id)
            .detail(serde_json::json!({ "email": body.email, "role": invited_role.as_str() })),
        )
        .await;

    Ok(Json(serde_json::json!({
        "token": token,
        // Not a URL. This route is a POST that takes the token in its body, so a `?token=` link would be a 405 for
        // anyone who clicked it — and a token in a URL ends up in access logs, `Referer` headers and browser history,
        // which is the one place a credential must never be.
        "accept_with": "POST /v1/vendor/invites/accept with { token, display_name, password }",
        "note": "this token is shown once; send it to the invitee yourself"
    })))
}

#[derive(Deserialize)]
struct AcceptBody {
    token: String,
    display_name: String,
    password: String,
}

/// `POST /v1/vendor/invites/accept` — public. Creates the account the invitation describes.
async fn accept(
    State(state): State<AppState>,
    peer: crate::limits::Peer,
    Json(body): Json<AcceptBody>,
) -> ApiResult<Json<serde_json::Value>> {
    // Public, and it hashes a token and looks one up: the one unauthenticated endpoint in the vendor surface, so it gets
    // the source budget. Only failures count, and a wrong token is a failure.
    let key = format!("rl:vendor-invite-accept:{}", peer.key());
    crate::limits::check(&state, &key, crate::limits::SOURCE).await?;

    if body.display_name.trim().is_empty() {
        return refuse(&state, &key, "a display name is required").await;
    }
    if body.password.chars().count() < 10 {
        return refuse(&state, &key, "choose a password of at least 10 characters").await;
    }

    let accepted = state
        .vendor_team
        .accept(&body.token, body.display_name.trim(), &body.password)
        .await;

    let (vendor_id, user_id) = match accepted {
        Ok(ids) => ids,
        Err(e) => {
            // A rejected invitation is the attempt worth counting: a wrong token, an expired one. A successful accept
            // is not, or one person clicking twice would spend the budget for their whole office.
            crate::limits::record(&state, &key, crate::limits::SOURCE).await;
            return Err(ApiError(e));
        }
    };

    Ok(Json(serde_json::json!({
        "vendor_id": vendor_id,
        "user_id": user_id,
        "next": "log in at /v1/vendor/auth/login"
    })))
}

/// A validation refusal that also counts against the source budget: a form filled fifty times with a wrong token is a
/// spray, and it should not be free just because the request was malformed.
async fn refuse(state: &AppState, key: &str, message: &str) -> ApiResult<Json<serde_json::Value>> {
    crate::limits::record(state, key, crate::limits::SOURCE).await;
    Err(ApiError(kernel::AppError::validation(message)))
}

/// `DELETE /v1/vendor/team/invites/{id}` — withdraw an invitation nobody has accepted.
async fn revoke_invite(
    State(state): State<AppState>,
    vendor: VendorAuth,
    peer: crate::limits::Peer,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state
        .vendor_team
        .revoke_invite(vendor.vendor_id, vendor.user_id, id)
        .await
        .map_err(ApiError)?;

    state
        .audit
        .record(
            domain::audit::AuditEntry::new(
                vendor.vendor_id,
                domain::audit::ActorKind::User,
                "team.invite_revoked",
                "vendor_invite",
            )
            .by(vendor.user_id)
            .on(id)
            .with_request(&vendor.request_id)
            .from_ip(peer.ip()),
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn remove(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    state
        .vendor_team
        .remove(vendor.vendor_id, vendor.user_id, id)
        .await
        .map_err(ApiError)?;

    state
        .audit
        .record(
            domain::audit::AuditEntry::new(
                vendor.vendor_id,
                domain::audit::ActorKind::User,
                "team.removed",
                "vendor_user",
            )
            .by(vendor.user_id)
            .on(id)
            .with_request(&vendor.request_id),
        )
        .await;

    Ok(Json(serde_json::json!({ "removed": id })))
}
