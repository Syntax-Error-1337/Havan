use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use domain::devices::Device;
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::Auth;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/devices", get(list))
        .route("/devices/{id}", get(one).patch(update).delete(remove))
}

#[derive(Deserialize)]
struct ListQuery {
    home_id: Uuid,
}

#[derive(Deserialize)]
struct UpdateBody {
    name: Option<String>,
    /// `Some(None)` clears the room; `None` leaves it alone.
    #[serde(default, deserialize_with = "double_option")]
    room_id: Option<Option<Uuid>>,
}

/// Distinguishes "field absent" from "field set to null" without a second DTO.
fn double_option<'de, D>(de: D) -> Result<Option<Option<Uuid>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Uuid>::deserialize(de).map(Some)
}

async fn list(
    State(state): State<AppState>,
    auth: Auth,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<Device>>> {
    Ok(Json(
        state
            .devices
            .list(q.home_id, auth.user.id)
            .await
            .map_err(ApiError)?,
    ))
}

async fn one(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Device>> {
    Ok(Json(
        state
            .devices
            .get(id, auth.user.id)
            .await
            .map_err(ApiError)?,
    ))
}

async fn update(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBody>,
) -> ApiResult<Json<Device>> {
    let mut device = state
        .devices
        .get(id, auth.user.id)
        .await
        .map_err(ApiError)?;

    if let Some(name) = body.name {
        device = state
            .devices
            .rename(id, auth.user.id, &name)
            .await
            .map_err(ApiError)?;
    }
    if let Some(room) = body.room_id {
        device = state
            .devices
            .assign_room(id, auth.user.id, room)
            .await
            .map_err(ApiError)?;
    }
    Ok(Json(device))
}

async fn remove(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state
        .devices
        .unclaim(id, auth.user.id)
        .await
        .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}
