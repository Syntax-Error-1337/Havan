use axum::Json;
use axum::Router;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use domain::notification::{Notification, NotificationRepo, PushRepo};
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
        // Where a phone tells the platform how to reach it. Without this, an alert is a row
        // in a table and a badge on a screen nobody is looking at.
        .route(
            "/notifications/tokens",
            post(register_token).delete(remove_token),
        )
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

#[derive(Deserialize)]
struct TokenBody {
    /// `ios` or `android`.
    platform: String,
    token: String,
    #[serde(default)]
    app_version: Option<String>,
}

/// `POST /notifications/tokens` — idempotent, because an app registers on every launch.
async fn register_token(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<TokenBody>,
) -> ApiResult<StatusCode> {
    // A token that is not a token is a bug in the app, and storing it would make the sender
    // fail on every sweep. Fail here instead, where the app can see it.
    if !matches!(body.platform.as_str(), "ios" | "android") {
        return Err(ApiError(kernel::AppError::validation(
            "platform must be ios or android",
        )));
    }
    let trimmed = body.token.trim();
    if trimmed.len() < 16 || trimmed.len() > 4096 {
        return Err(ApiError(kernel::AppError::validation(
            "token is not a plausible device token",
        )));
    }

    state
        .notifications
        .register_token(
            auth.user.id,
            &body.platform,
            trimmed,
            body.app_version.as_deref(),
        )
        .await
        .map_err(ApiError)?;

    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /notifications/tokens/{token}` — called on sign-out, so the next person to hold
/// the phone does not receive the previous owner's alerts.
async fn remove_token(
    State(state): State<AppState>,
    auth: Auth,
    Path(token): Path<String>,
) -> ApiResult<StatusCode> {
    let removed = state
        .notifications
        .remove_token(auth.user.id, &token)
        .await
        .map_err(ApiError)?;

    Ok(if removed {
        StatusCode::NO_CONTENT
    } else {
        // Removing a token that is not there is what a second sign-out looks like.
        StatusCode::NO_CONTENT
    })
}
