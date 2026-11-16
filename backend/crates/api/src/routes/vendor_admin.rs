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

    Ok(Json(state.vendors.patch(id, &body).await.map_err(ApiError)?))
}
