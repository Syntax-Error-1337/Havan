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
        .route("/me/exports", get(receipts))
        .route("/me", delete(erase))
        .route("/ops/privacy/purge", axum::routing::post(purge))
        .route("/ops/privacy/due", axum::routing::get(due))
}

/// `GET /v1/ops/privacy/due` — how many erased accounts are past their retention window.
async fn due(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    peer: crate::limits::Peer,
) -> ApiResult<Json<serde_json::Value>> {
    super::ops::require_ops(&state, &headers, &peer)?;
    Ok(Json(
        serde_json::json!({ "due": state.privacy.purging_soon().await.map_err(ApiError)? }),
    ))
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

/// `GET /v1/me/exports` — the receipts. What a subject-access request is answered with, once the file is gone.
async fn receipts(State(state): State<AppState>, user: Auth) -> ApiResult<Json<serde_json::Value>> {
    let receipts = state
        .privacy
        .exports(user.user.id)
        .await
        .map_err(ApiError)?;
    Ok(Json(serde_json::json!({
        "count": receipts.len(),
        "exports": receipts,
    })))
}

async fn export(State(state): State<AppState>, user: Auth) -> ApiResult<impl IntoResponse> {
    let bundle: ExportBundle = state.privacy.export(user.user.id).await.map_err(ApiError)?;

    // Serialise once: the size in the receipt has to be the size of what was handed over, and serialising twice is a
    // second chance for the two to disagree.
    let body = serde_json::to_vec(&bundle).map_err(|e| {
        ApiError(kernel::AppError::Internal(format!(
            "export serialisation: {e}"
        )))
    })?;

    // Best effort: a download that fails because its receipt could not be written would be a bad trade. The failure is
    // logged, and the person still gets their data.
    if let Err(e) = state
        .privacy
        .record_export(user.user.id, body.len() as i64)
        .await
    {
        tracing::warn!(error = ?e, user = %user.user.id, "could not record the export receipt");
    }

    // A date in the filename rather than an id: the person recognises the file, and a shared
    // download folder does not leak who they are to anyone scanning names.
    let filename = format!(
        "haven-export-{}.json",
        bundle.exported_at.date().to_string().replace('-', "")
    );

    Ok((
        [
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
            (header::CONTENT_TYPE, "application/json".to_string()),
        ],
        body,
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
