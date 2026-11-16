//! Certification: the vendor asks, the operator signs off.
//!
//! Publishing is a vendor's promise that the product is done; certification is our statement
//! that we tested it. Only a certified product can be claimed in our app — otherwise a
//! vendor's unfinished device appears in somebody's home under our name, and the support
//! ticket is ours.
//!
//! The state machine itself lives in `domain::catalog::certification_transition`, so the
//! rules are testable without a database or a router.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::routing::post;
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::VendorAuth;
use crate::error::{ApiError, ApiResult};
use crate::limits::Peer;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/vendor/products/{id}/certification/submit", post(submit))
        .route("/ops/products/{id}/certification/certify", post(certify))
        .route("/ops/products/{id}/certification/revoke", post(revoke))
}

#[derive(Deserialize, Default)]
struct NoteBody {
    #[serde(default)]
    note: Option<String>,
}

/// `POST /v1/vendor/products/{id}/certification/submit` — "we think this is ready".
///
/// The vendor can only submit its own product: the route is vendor-authenticated, and the
/// repository runs in that vendor's context, so RLS decides what "own" means.
async fn submit(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<NoteBody>,
) -> ApiResult<Json<serde_json::Value>> {
    state
        .catalog
        .set_certification(
            Some(vendor.vendor_id),
            id,
            "submitted",
            body.note.as_deref(),
        )
        .await
        .map_err(ApiError)?;

    state
        .audit
        .record(
            domain::audit::AuditEntry::new(
                vendor.vendor_id,
                domain::audit::ActorKind::User,
                "product.certification_submitted",
                "product",
            )
            .by(vendor.user_id)
            .on(id)
            .with_request(&vendor.request_id),
        )
        .await;

    Ok(Json(
        serde_json::json!({ "id": id, "certification": "submitted" }),
    ))
}

/// `POST /v1/ops/products/{id}/certification/certify` — the sign-off that lets a device into
/// the app. Operator only, because it is our name on the product.
async fn certify(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
    Path(id): Path<Uuid>,
    Json(body): Json<NoteBody>,
) -> ApiResult<Json<serde_json::Value>> {
    crate::routes::ops::require_ops(&state, &headers, &peer)?;

    state
        .catalog
        .set_certification(None, id, "certified", body.note.as_deref())
        .await
        .map_err(ApiError)?;

    Ok(Json(
        serde_json::json!({ "id": id, "certification": "certified" }),
    ))
}

/// `POST /v1/ops/products/{id}/certification/revoke` — we were wrong, or the vendor changed
/// something. Devices already claimed keep working; new ones cannot be added.
async fn revoke(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
    Path(id): Path<Uuid>,
    Json(body): Json<NoteBody>,
) -> ApiResult<Json<serde_json::Value>> {
    crate::routes::ops::require_ops(&state, &headers, &peer)?;

    state
        .catalog
        .set_certification(None, id, "revoked", body.note.as_deref())
        .await
        .map_err(ApiError)?;

    Ok(Json(
        serde_json::json!({ "id": id, "certification": "revoked" }),
    ))
}
