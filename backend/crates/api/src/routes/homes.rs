use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use domain::homes::{Home, Member, Role, Room};
use domain::notification::NotificationRepo;
use domain::scenes::SceneRepo;
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
        // Everything a home screen draws, in one round trip. Five requests to paint
        // one screen is five chances to be half-painted on a phone.
        .route("/homes/{id}/dashboard", get(dashboard))
        .route("/rooms/{room_id}", patch(rename_room).delete(delete_room))
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

/// What the home screen needs, in one call: the home, its rooms, its devices with
/// their points, its scenes, and the caller's unread count.
///
/// Each part is read through the service that owns it, so the tenancy checks are the
/// same ones the individual endpoints use — this endpoint inherits the policies
/// rather than re-implementing them.
#[derive(serde::Serialize)]
struct Dashboard {
    home: Home,
    rooms: Vec<Room>,
    devices: Vec<domain::devices::Device>,
    scenes: Vec<domain::scenes::Scene>,
    unread_count: i64,
}

async fn dashboard(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Dashboard>> {
    // The home check comes first and fails with 404 for a stranger, so the rest
    // can assume membership.
    let home = state
        .homes
        .require(id, auth.user.id)
        .await
        .map_err(ApiError)?;

    let rooms = state
        .homes
        .rooms(id, auth.user.id)
        .await
        .map_err(ApiError)?;
    let devices = state
        .devices
        .list(id, auth.user.id)
        .await
        .map_err(ApiError)?;
    let scenes = state
        .scenes
        .list(id, auth.user.id)
        .await
        .map_err(ApiError)?;
    let unread_count = state
        .notifications
        .unread_count(auth.user.id)
        .await
        .map_err(ApiError)?;

    Ok(Json(Dashboard {
        home,
        rooms,
        devices,
        scenes,
        unread_count,
    }))
}

#[derive(Deserialize)]
struct RenameRoom {
    name: String,
}

async fn rename_room(
    State(state): State<AppState>,
    auth: Auth,
    Path(room_id): Path<Uuid>,
    axum::extract::Query(q): axum::extract::Query<RoomQuery>,
    Json(body): Json<RenameRoom>,
) -> ApiResult<StatusCode> {
    state
        .homes
        .rename_room(room_id, q.home_id, auth.user.id, &body.name)
        .await
        .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
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
