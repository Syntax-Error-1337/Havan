//! Data subject requests: `GET /v1/me/export` and `DELETE /v1/me`.
//!
//! Both act on the caller and nobody else — no id in the path, so there is no id to guess. The
//! export is offered as a download because a person asking for their data wants a file, not a
//! JSON blob in a browser tab.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{delete, get};
use domain::privacy::{ErasureReport, ExportBundle, PrivacyRepo};

use crate::app::AppState;
use crate::auth::Auth;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me/export", get(export))
        .route("/me", delete(erase))
        .route("/ops/privacy/purge", axum::routing::post(purge))
}

/// `POST /v1/ops/privacy/purge`
///
/// The worker runs this hourly; the door exists so an operator can prove it works without waiting
/// for a tick, and so a backlog can be cleared by hand after an outage. Safe to call repeatedly:
/// a tombstone that is not due is not touched.
async fn purge(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    peer: crate::limits::Peer,
) -> ApiResult<Json<domain::privacy::PurgeReport>> {
    super::ops::require_ops(&state, &headers, &peer)?;

    // Batched, so one request cannot hold a transaction open over an unbounded backlog.
    Ok(Json(state.privacy.purge_due(500).await.map_err(ApiError)?))
}

async fn export(State(state): State<AppState>, user: Auth) -> ApiResult<impl IntoResponse> {
    let bundle: ExportBundle = state.privacy.export(user.user.id).await.map_err(ApiError)?;

    // A date in the filename rather than an id: the person recognises the file, and a shared
    // download folder does not leak who they are to anyone scanning names.
    let filename = format!(
        "haven-export-{}.json",
        bundle.exported_at.date().to_string().replace('-', "")
    );

    Ok((
        [(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )],
        Json(bundle),
    ))
}

/// `DELETE /v1/me` — erase this account.
///
/// Returns a receipt rather than an empty body: "we removed 3 devices" is what the person, and
/// anyone auditing later, needs to see. The caller's tokens stop working immediately because every
/// refresh token is deleted in the same transaction; the access token expires on its own.
async fn erase(State(state): State<AppState>, user: Auth) -> ApiResult<Json<ErasureReport>> {
    let report = state.privacy.erase(user.user.id).await.map_err(ApiError)?;

    tracing::info!(
        user = %user.user.id,
        homes = report.homes_deleted,
        devices = report.devices_deleted,
        sessions = report.sessions_revoked,
        "account erased at the account holder's request"
    );

    Ok(Json(report))
}
