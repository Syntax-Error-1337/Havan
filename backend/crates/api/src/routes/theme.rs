//! Platform appearance: the one place a white-label operator changes the product's colours.
//!
//! `GET /v1/theme` is deliberately unauthenticated — the sign-in screen has to be branded before
//! there is a session, and a theme is not a secret. It is cached briefly so a cold start on a train
//! does not wait for it, and every write is behind the operator door.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;

use domain::theme::{AppTheme, ThemeInput};

use crate::app::AppState;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        // The app's door: no session, no tenant, the same answer for everyone.
        .route("/theme", get(public_theme))
        .route("/ops/theme", get(ops_theme).put(ops_save))
        .route("/ops/theme/history", get(ops_history))
}

/// `GET /v1/theme`
///
/// Public on purpose, and cached for a minute: a phone that starts offline keeps the theme it last
/// saw, and one that starts online spends no round trip doing it.
async fn public_theme(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<impl IntoResponse> {
    let theme = state.theme.load().await.map_err(ApiError)?;

    // A weak validator derived from what an operator changes: the row's `updated_at`, or the values
    // themselves when the row has never been written. A million phones asking on launch should be
    // answering "nothing changed" with 304 and no body, not re-downloading a palette they already have.
    let etag = etag_for(&theme);

    let unchanged = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|sent| sent.split(',').any(|candidate| candidate.trim() == etag));

    let cache = [
        (
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=60"),
        ),
        // The tag is hex and two quotes, so this cannot fail; if it somehow did, that is our bug and
        // a 500 is the honest answer rather than serving a theme with no validator.
        (
            header::ETAG,
            HeaderValue::from_str(&etag).map_err(|_| {
                ApiError(kernel::AppError::code(
                    kernel::ErrorCode::Internal,
                    "could not build the theme validator",
                ))
            })?,
        ),
    ];

    if unchanged {
        return Ok((
            StatusCode::NOT_MODIFIED,
            cache,
            Json(serde_json::Value::Null),
        )
            .into_response());
    }

    Ok((StatusCode::OK, cache, Json(theme)).into_response())
}

/// `W/"<16 hex>"` — weak because the body's bytes are not canonical JSON, only its meaning is.
fn etag_for(theme: &AppTheme) -> String {
    let seed = match theme.updated_at {
        Some(at) => format!("{}:{}", at.unix_timestamp_nanos(), theme.accent),
        None => format!(
            "{}:{}:{}:{}",
            theme.product_name,
            theme.accent,
            theme.accent_secondary,
            theme.mode.as_str()
        ),
    };
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("W/\"{hash:016x}\"")
}

/// `GET /v1/ops/theme` — the console reads the same row the app does, so the preview cannot drift.
async fn ops_theme(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: crate::limits::Peer,
) -> ApiResult<Json<AppTheme>> {
    super::ops::require_ops(&state, &headers, &peer)?;
    Ok(Json(state.theme.load().await.map_err(ApiError)?))
}

/// `PUT /v1/ops/theme`
///
/// Validation lives in the domain, so the console and the API cannot disagree about what a colour is.
/// The actor comes from `x-ops-actor` when the caller supplies one — an operator console knows who is
/// signed in, a curl does not — and falls back to `ops` rather than an empty string, because a
/// history row that says nobody made the change is worse than one that says "ops".
async fn ops_save(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: crate::limits::Peer,
    Json(input): Json<ThemeInput>,
) -> ApiResult<Json<AppTheme>> {
    super::ops::require_ops(&state, &headers, &peer)?;

    let requested = AppTheme::from_input(&input).map_err(ApiError)?;
    let actor = headers
        .get("x-ops-actor")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("ops");

    let saved = state
        .theme
        .save(&requested, actor)
        .await
        .map_err(ApiError)?;
    Ok(Json(saved))
}

/// `GET /v1/ops/theme/history` — what changed, when, and by whom.
async fn ops_history(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: crate::limits::Peer,
) -> ApiResult<Json<serde_json::Value>> {
    super::ops::require_ops(&state, &headers, &peer)?;
    let changes = state.theme.history(20).await.map_err(ApiError)?;
    Ok(Json(serde_json::json!({ "changes": changes })))
}
