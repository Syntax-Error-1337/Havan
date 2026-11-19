use axum::Json;
use axum::Router;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
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
        .route("/notifications/{id}", delete(dismiss))
        // Where a phone tells the platform how to reach it. Without this, an alert is a row
        // in a table and a badge on a screen nobody is looking at.
        // Register on the bare path, remove by path segment: the app calls
        // `DELETE /notifications/tokens/{token}` on sign-out, and a handler that extracts `Path`
        // from a route with no parameter answers 500 with axum's own words in the body. The route
        // and the handler disagreed, and only a request could tell.
        .route(
            "/notifications/tokens",
            post(register_token)
                .get(list_tokens)
                .delete(forget_all_tokens),
        )
        .route("/notifications/tokens/{token}", delete(remove_token))
        .route(
            "/notifications/quiet-hours",
            get(quiet_hours).put(set_quiet_hours),
        )
}

/// `GET /v1/notifications/quiet-hours` — the window in which pushes are held.
async fn quiet_hours(
    State(state): State<AppState>,
    auth: Auth,
) -> ApiResult<Json<domain::notification::QuietHours>> {
    Ok(Json(
        state
            .notifications
            .quiet_hours(auth.user.id)
            .await
            .map_err(ApiError)?,
    ))
}

/// `PUT /v1/notifications/quiet-hours` — set or clear it.
///
/// Refuses three things the app could otherwise store happily and the platform could not honour: a time that is not
/// `HH:MM`, an end equal to the start (all day, or never — ambiguous), and a zone Postgres does not know, which would
/// silently never match and quietly turn quiet hours off.
async fn set_quiet_hours(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<domain::notification::QuietHours>,
) -> ApiResult<Json<domain::notification::QuietHours>> {
    if let (Some(from), Some(to)) = (&body.from, &body.to) {
        let from = parse_clock(from)?;
        let to = parse_clock(to)?;
        if from == to {
            return Err(ApiError(kernel::AppError::validation(
                "quiet hours need two different times — the same time is either all day or never",
            )));
        }
        let timezone = body
            .timezone
            .as_deref()
            .ok_or_else(|| ApiError(kernel::AppError::validation("quiet hours need a timezone")))?;
        if !state
            .notifications
            .timezone_exists(timezone)
            .await
            .map_err(ApiError)?
        {
            return Err(ApiError(kernel::AppError::validation(format!(
                "unknown timezone '{timezone}'"
            ))));
        }
    } else if body.from.is_some() || body.to.is_some() || body.timezone.is_some() {
        return Err(ApiError(kernel::AppError::validation(
            "quiet hours need a start, an end and a timezone, or none of them",
        )));
    }

    state
        .notifications
        .set_quiet_hours(auth.user.id, &body)
        .await
        .map_err(ApiError)?;

    // Answer with what was stored, read back: a client that sent 9:5 and got 09:05 back learns the format it will see.
    Ok(Json(
        state
            .notifications
            .quiet_hours(auth.user.id)
            .await
            .map_err(ApiError)?,
    ))
}

/// `HH:MM`, 24-hour. Not `9:5`, not `09:05:00` — one shape, so the value the app reads back is the value it sent.
fn parse_clock(value: &str) -> ApiResult<(u8, u8)> {
    let (hour, minute) = value.split_once(':').ok_or_else(|| {
        ApiError(kernel::AppError::validation(
            "a time looks like HH:MM, 24-hour",
        ))
    })?;
    if hour.len() != 2 || minute.len() != 2 {
        return Err(ApiError(kernel::AppError::validation(
            "a time looks like HH:MM, 24-hour — two digits each",
        )));
    }
    let hour: u8 = hour
        .parse()
        .map_err(|_| ApiError(kernel::AppError::validation("bad hour")))?;
    let minute: u8 = minute
        .parse()
        .map_err(|_| ApiError(kernel::AppError::validation("bad minute")))?;
    if hour > 23 || minute > 59 {
        return Err(ApiError(kernel::AppError::validation(
            "that is not a time of day",
        )));
    }
    Ok((hour, minute))
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

async fn dismiss(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let removed = state
        .notifications
        .dismiss(auth.user.id, id)
        .await
        .map_err(ApiError)?;

    // Already gone is not an error: two phones dismissing the same alert, or a retry on a flaky link, must not
    // look like a failure to the person who asked.
    Ok(Json(serde_json::json!({ "removed": removed })))
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
/// The phones that will receive this person's alerts.
///
/// Registration existed and nothing could read it back, so Settings could not answer "is this phone going to buzz?" —
/// and the usual reason somebody asks is that it is not.
async fn list_tokens(
    State(state): State<AppState>,
    auth: Auth,
) -> ApiResult<Json<serde_json::Value>> {
    let tokens = state
        .notifications
        .tokens_for(auth.user.id)
        .await
        .map_err(ApiError)?;

    // The token itself is not echoed back: it is not a secret, but it is a delivery address, and a screen has no use
    // for it. Platform and app version are enough to tell two phones apart.
    let devices: Vec<serde_json::Value> = tokens
        .into_iter()
        .map(|t| {
            serde_json::json!({
                "platform": t.platform,
                "app_version": t.app_version,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "count": devices.len(),
        "devices": devices,
    })))
}

/// Stop every phone from receiving alerts for this account.
async fn forget_all_tokens(
    State(state): State<AppState>,
    auth: Auth,
) -> ApiResult<Json<serde_json::Value>> {
    let tokens = state
        .notifications
        .tokens_for(auth.user.id)
        .await
        .map_err(ApiError)?;

    let mut off = 0u64;
    // One row at a time on purpose: `tokens_for` is bounded by how many phones one person owns, and a bulk DELETE that
    // skipped a row would leave the screen saying "off" while a phone kept buzzing.
    for token in tokens {
        if state
            .notifications
            .remove_token(auth.user.id, &token.token)
            .await
            .map_err(ApiError)?
        {
            off += 1;
        }
    }

    Ok(Json(serde_json::json!({ "turned_off": off })))
}

async fn remove_token(
    State(state): State<AppState>,
    auth: Auth,
    Path(token): Path<String>,
) -> ApiResult<StatusCode> {
    // A token that is not there is what a second sign-out looks like, so the answer is the same
    // either way — an app calling this after its row is already gone has done nothing wrong.
    let _ = state
        .notifications
        .remove_token(auth.user.id, &token)
        .await
        .map_err(ApiError)?;

    Ok(StatusCode::NO_CONTENT)
}
