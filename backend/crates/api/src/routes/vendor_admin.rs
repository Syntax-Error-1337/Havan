//! The operator's console: the vendors on this platform, and whether they may sell.

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::routing::get;
use domain::vendor_admin::{NewVendor, VendorAdminRepo, VendorPatch, VendorRow};
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::error::{ApiError, ApiResult};
use crate::limits::Peer;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ops/vendors", get(list).post(create))
        .route("/ops/vendors/{id}", get(one).patch(patch))
        .route("/ops/certification/queue", get(queue))
        .route("/ops/overview", get(overview))
        .route("/ops/health", get(health))
        .route("/ops/audit", get(audit))
}

/// `GET /v1/ops/certification/queue` — products waiting for our sign-off, oldest first.
///
/// Cross-vendor, which is exactly why it lives behind the ops door: no vendor may learn that another
/// vendor's product exists.
async fn queue(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
) -> ApiResult<Json<Vec<infra::repos::cert_queue::QueueItem>>> {
    super::ops::require_ops(&state, &headers, &peer)?;

    Ok(Json(state.cert_queue.pending(100).await.map_err(ApiError)?))
}

/// `GET /v1/ops/health` — the platform's own view of itself, for the console's health page.
async fn health(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
) -> ApiResult<Json<infra::repos::platform_health::PlatformHealth>> {
    super::ops::require_ops(&state, &headers, &peer)?;

    Ok(Json(state.health.read().await.map_err(ApiError)?))
}

#[derive(serde::Deserialize)]
struct AuditQuery {
    #[serde(default)]
    vendor: Option<Uuid>,
    #[serde(default)]
    before: Option<Uuid>,
    #[serde(default = "audit_limit")]
    limit: i64,
}

fn audit_limit() -> i64 {
    100
}

/// `GET /v1/ops/audit` — who did what, across every vendor.
///
/// The operator can suspend a vendor, certify a product and grant licences, and until now the record
/// of that was readable only per vendor: the console could not answer "who signed this off". Same
/// rows the vendor sees, widened — so the two answers cannot disagree.
async fn audit(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
    axum::extract::Query(q): axum::extract::Query<AuditQuery>,
) -> ApiResult<Json<domain::audit::AuditPage>> {
    super::ops::require_ops(&state, &headers, &peer)?;

    Ok(Json(
        state
            .audit
            .page_all(q.vendor, q.before, Some(q.limit.clamp(1, 200)))
            .await
            .map_err(ApiError)?,
    ))
}

/// `GET /v1/ops/overview` — the four numbers the console opens with.
async fn overview(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
) -> ApiResult<Json<infra::repos::cert_queue::Overview>> {
    super::ops::require_ops(&state, &headers, &peer)?;

    Ok(Json(state.cert_queue.overview().await.map_err(ApiError)?))
}

#[derive(Deserialize)]
struct ListQuery {
    limit: Option<i64>,
}

/// `GET /v1/ops/vendors` — every vendor, with the numbers an operator asks about first.
async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<VendorRow>>> {
    super::ops::require_ops(&state, &headers, &peer)?;

    Ok(Json(
        state
            .vendors
            .list(q.limit.unwrap_or(100))
            .await
            .map_err(ApiError)?,
    ))
}

async fn one(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<VendorRow>> {
    super::ops::require_ops(&state, &headers, &peer)?;

    Ok(Json(state.vendors.one(id).await.map_err(ApiError)?))
}

/// `POST /v1/ops/vendors` — onboard a vendor.
async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
    Json(body): Json<NewVendor>,
) -> ApiResult<Json<VendorRow>> {
    super::ops::require_ops(&state, &headers, &peer)?;

    Ok(Json(state.vendors.create(&body).await.map_err(ApiError)?))
}

/// `PATCH /v1/ops/vendors/{id}` — suspend, resume, set a quota, rename.
///
/// Suspension does not touch a single device row: devices already in people's homes keep working,
/// new activations stop, and the vendor's portal becomes read-only. That is the clause in docs/11,
/// and the reason it is implemented here rather than as a delete.
async fn patch(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
    Path(id): Path<Uuid>,
    Json(body): Json<VendorPatch>,
) -> ApiResult<Json<VendorRow>> {
    super::ops::require_ops(&state, &headers, &peer)?;

    Ok(Json(
        state.vendors.patch(id, &body).await.map_err(ApiError)?,
    ))
}
