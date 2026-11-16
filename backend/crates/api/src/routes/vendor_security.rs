//! Two-factor for the portal: the policy, the enrolment, and the member's own secret.

use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::routing::{delete, get, post};
use domain::vendor_security::{Enrolment, SecurityPolicy, VendorSecurityRepo};
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::VendorAuth;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/vendor/security", get(read).patch(configure))
        .route("/vendor/totp/enrol", post(enrol))
        .route("/vendor/totp/confirm", post(confirm))
        .route("/vendor/totp", delete(disable))
}

async fn read(
    State(state): State<AppState>,
    vendor: VendorAuth,
) -> ApiResult<Json<SecurityPolicy>> {
    Ok(Json(
        state
            .security
            .policy(vendor.vendor_id)
            .await
            .map_err(ApiError)?,
    ))
}

#[derive(Deserialize)]
struct PolicyBody {
    totp_required: bool,
}

/// `PATCH /v1/vendor/security` — owner only, and refused while anybody has not enrolled.
async fn configure(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Json(body): Json<PolicyBody>,
) -> ApiResult<Json<SecurityPolicy>> {
    Ok(Json(
        state
            .security
            .set_policy(vendor.vendor_id, vendor.user_id, body.totp_required)
            .await
            .map_err(ApiError)?,
    ))
}

/// `POST /v1/vendor/totp/enrol` — a new secret, shown once, with the URI to scan.
///
/// Enrolling again issues a fresh secret and clears the confirmation, so a half-finished enrolment
/// never counts as a factor.
async fn enrol(State(state): State<AppState>, vendor: VendorAuth) -> ApiResult<Json<Enrolment>> {
    Ok(Json(
        state
            .security
            .enrol(vendor.vendor_id, vendor.user_id)
            .await
            .map_err(ApiError)?,
    ))
}

#[derive(Deserialize)]
struct ConfirmBody {
    code: String,
}

/// `POST /v1/vendor/totp/confirm` — prove you can read a code from the secret.
async fn confirm(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Json(body): Json<ConfirmBody>,
) -> ApiResult<Json<SecurityPolicy>> {
    Ok(Json(
        state
            .security
            .confirm(vendor.vendor_id, vendor.user_id, &body.code)
            .await
            .map_err(ApiError)?,
    ))
}

/// `DELETE /v1/vendor/totp` — off, unless the vendor requires it.
async fn disable(
    State(state): State<AppState>,
    vendor: VendorAuth,
) -> ApiResult<Json<SecurityPolicy>> {
    Ok(Json(
        state
            .security
            .disable(vendor.vendor_id, vendor.user_id)
            .await
            .map_err(ApiError)?,
    ))
}
