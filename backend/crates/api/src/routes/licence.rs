//! Licences: the vendor's balance, and the operator's grant.
//!
//! Two audiences, one table. A vendor reads its own balance and the serials it has
//! activated — never who activated them. An operator grants activations to a vendor it is
//! not, which is why the grant is a separate route behind the ops token rather than a
//! vendor-scoped one no vendor should be able to call.

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::routing::get;
use domain::licence::{ActivationRow, LicenceRepo, LicenceSummary, NewGrant};
use serde::Serialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::VendorAuth;
use crate::error::{ApiError, ApiResult};
use crate::limits::Peer;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/vendor/licences", get(mine))
        .route("/ops/licences", get(for_vendor).post(grant))
}

#[derive(serde::Deserialize)]
struct MineQuery {
    limit: Option<i64>,
}

#[derive(Serialize)]
struct LicencePage {
    summary: LicenceSummary,
    activations: Vec<ActivationRow>,
}

/// `GET /v1/vendor/licences` — what we bought, what we used, and the devices behind it.
async fn mine(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Query(q): Query<MineQuery>,
) -> ApiResult<Json<LicencePage>> {
    let summary = state
        .licences
        .summary(vendor.vendor_id)
        .await
        .map_err(ApiError)?;
    let activations = state
        .licences
        .activations(vendor.vendor_id, q.limit.unwrap_or(50))
        .await
        .map_err(ApiError)?;

    Ok(Json(LicencePage {
        summary,
        activations,
    }))
}

#[derive(serde::Deserialize)]
struct VendorQuery {
    vendor_id: Uuid,
    limit: Option<i64>,
}

/// `GET /v1/ops/licences?vendor_id=` — the same view, for whoever bills the vendor.
async fn for_vendor(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
    Query(q): Query<VendorQuery>,
) -> ApiResult<Json<LicencePage>> {
    crate::routes::ops::require_ops(&state, &headers, &peer)?;

    let summary = state
        .licences
        .summary(q.vendor_id)
        .await
        .map_err(ApiError)?;
    let activations = state
        .licences
        .activations(q.vendor_id, q.limit.unwrap_or(100))
        .await
        .map_err(ApiError)?;

    Ok(Json(LicencePage {
        summary,
        activations,
    }))
}

/// `POST /v1/ops/licences` — authorise more activations.
///
/// Returns the grant id and the resulting balance, so an operator sees the effect rather
/// than a 200 that means nothing.
async fn grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
    Json(body): Json<NewGrant>,
) -> ApiResult<Json<serde_json::Value>> {
    crate::routes::ops::require_ops(&state, &headers, &peer)?;

    let id = state.licences.grant(&body).await.map_err(ApiError)?;

    let summary = state
        .licences
        .summary(body.vendor_id)
        .await
        .map_err(ApiError)?;

    // How an operator answers "who gave this vendor 10,000 activations for free" six
    // months from now.
    state
        .audit
        .record(
            domain::audit::AuditEntry::new(
                body.vendor_id,
                domain::audit::ActorKind::System,
                "licence.granted",
                "licence_grant",
            )
            .on(id)
            .from_ip(peer.ip())
            .detail(serde_json::json!({
                "quantity": body.quantity,
                "unit_price_cents": body.unit_price_cents,
                "currency": body.currency(),
                "expires_in_days": body.expires_in_days,
            })),
        )
        .await;

    Ok(Json(serde_json::json!({ "id": id, "summary": summary })))
}
