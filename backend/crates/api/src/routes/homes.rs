use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use domain::homes::{Home, Member, Role, Room};
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::Auth;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/homes", get(list).post(create))
        .route("/homes/{id}", get(one).patch(rename).delete(remove))
        .route("/homes/{id}/members", get(members).post(invite))
        .route(
            "/homes/{id}/members/{user_id}",
            patch(set_role).delete(remove_member),
        )
        .route("/homes/{id}/rooms", get(rooms).post(create_room))
        .route("/rooms/{room_id}", axum::routing::delete(delete_room))
}

#[derive(Deserialize)]
struct CreateHome {
    name: String,
    #[serde(default = "default_tz")]
    timezone: String,
}

fn default_tz() -> String {
    "UTC".to_string()
}

#[derive(Deserialize)]
struct RenameHome {
    name: String,
}

#[derive(Deserialize)]
struct InviteBody {
    email: String,
    #[serde(default = "default_role")]
    role: String,
}

fn default_role() -> String {
    "member".to_string()
}

#[derive(Deserialize)]
struct RoleBody {
    role: String,
}

#[derive(Deserialize)]
struct CreateRoom {
    name: String,
    icon: Option<String>,
}

#[derive(Deserialize)]
struct RoomQuery {
    home_id: Uuid,
}

async fn list(State(state): State<AppState>, auth: Auth) -> ApiResult<Json<Vec<Home>>> {
    Ok(Json(
        state.homes.list(auth.user.id).await.map_err(ApiError)?,
    ))
}

async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<CreateHome>,
) -> ApiResult<(StatusCode, Json<Home>)> {
    let home = state
        .homes
        .create(auth.user.id, &body.name, &body.timezone)
        .await
        .map_err(ApiError)?;
    Ok((StatusCode::CREATED, Json(home)))
}

async fn one(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Home>> {
    Ok(Json(
        state
            .homes
            .require(id, auth.user.id)
            .await
            .map_err(ApiError)?,
    ))
}

async fn rename(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<RenameHome>,
) -> ApiResult<Json<Home>> {
    Ok(Json(
        state
            .homes
            .rename(id, auth.user.id, &body.name)
            .await
            .map_err(ApiError)?,
    ))
}

async fn remove(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state
        .homes
        .delete(id, auth.user.id)
        .await
        .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn members(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<Member>>> {
    Ok(Json(
        state
            .homes
            .members(id, auth.user.id)
            .await
            .map_err(ApiError)?,
    ))
}

async fn invite(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<InviteBody>,
) -> ApiResult<StatusCode> {
    let role = parse_role(&body.role)?;
    state
        .homes
        .invite(id, auth.user.id, &body.email, role)
        .await
        .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn set_role(
    State(state): State<AppState>,
    auth: Auth,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<RoleBody>,
) -> ApiResult<StatusCode> {
    state
        .homes
        .set_role(id, auth.user.id, user_id, parse_role(&body.role)?)
        .await
        .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_member(
    State(state): State<AppState>,
    auth: Auth,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    state
        .homes
        .remove_member(id, auth.user.id, user_id)
        .await
        .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn rooms(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<Room>>> {
    Ok(Json(
        state
            .homes
            .rooms(id, auth.user.id)
            .await
            .map_err(ApiError)?,
    ))
}

async fn create_room(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateRoom>,
) -> ApiResult<(StatusCode, Json<Room>)> {
    let room = state
        .homes
        .create_room(id, auth.user.id, &body.name, body.icon.as_deref())
        .await
        .map_err(ApiError)?;
    Ok((StatusCode::CREATED, Json(room)))
}

async fn delete_room(
    State(state): State<AppState>,
    auth: Auth,
    Path(room_id): Path<Uuid>,
    axum::extract::Query(q): axum::extract::Query<RoomQuery>,
) -> ApiResult<StatusCode> {
    state
        .homes
        .delete_room(room_id, q.home_id, auth.user.id)
        .await
        .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_role(s: &str) -> Result<Role, ApiError> {
    Role::parse(s).ok_or_else(|| {
        ApiError(kernel::AppError::validation(
            "role must be one of guest, member, admin",
        ))
    })
}
