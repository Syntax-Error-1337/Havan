use axum::Json;
use axum::Router;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use domain::notification::{Notification, NotificationRepo};
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::Auth;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/notifications", get(list))
        .route("/notifications/unread-count", get(unread_count))
        .route("/notifications/read-all", post(read_all))
        .route("/notifications/{id}/read", post(mark_read))
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    unread: bool,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    50
}

async fn list(
    State(state): State<AppState>,
    auth: Auth,
    q: Result<Query<ListQuery>, QueryRejection>,
) -> ApiResult<Json<Vec<Notification>>> {
    let Query(q) = q.map_err(|e| ApiError(kernel::AppError::validation(e.body_text())))?;

    Ok(Json(
        state
            .notifications
            .list(auth.user.id, q.unread, q.limit)
            .await
            .map_err(ApiError)?,
    ))
}

async fn unread_count(
    State(state): State<AppState>,
    auth: Auth,
) -> ApiResult<Json<serde_json::Value>> {
    let count = state
        .notifications
        .unread_count(auth.user.id)
        .await
        .map_err(ApiError)?;
    Ok(Json(serde_json::json!({ "unread": count })))
}

async fn mark_read(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let changed = state
        .notifications
        .mark_read(auth.user.id, id)
        .await
        .map_err(ApiError)?;

    // `changed: false` is a second phone opening the same alert — not an error,
    // and the client can tell the difference.
    Ok(Json(serde_json::json!({ "changed": changed })))
}

async fn read_all(State(state): State<AppState>, auth: Auth) -> ApiResult<Json<serde_json::Value>> {
    let cleared = state
        .notifications
        .mark_all_read(auth.user.id)
        .await
        .map_err(ApiError)?;
    Ok(Json(serde_json::json!({ "cleared": cleared })))
}
