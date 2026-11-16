//! SSO configuration endpoints.
//!
//! `PATCH /v1/vendor/sso` records the vendor's intent and their email domain; `POST
//! /v1/vendor/team/{id}/sso` binds an IdP identity to an account. Both report `enforced: false`
//! until assertion validation exists, because telling a vendor they are protected when they are not
//! is the one outcome worse than the gap.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use domain::sso::{SsoConfig, SsoRepo};
use domain::vendor_team::VendorTeamRepo;
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::VendorAuth;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/vendor/sso", get(config).patch(configure))
        .route("/vendor/team/{id}/sso", post(bind))
}

async fn config(State(state): State<AppState>, vendor: VendorAuth) -> ApiResult<Json<SsoConfig>> {
    Ok(Json(
        state.sso.config(vendor.vendor_id).await.map_err(ApiError)?,
    ))
}

#[derive(Deserialize)]
struct ConfigureBody {
    required: bool,
    #[serde(default)]
    domain: Option<String>,
}

async fn configure(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Json(body): Json<ConfigureBody>,
) -> ApiResult<Json<SsoConfig>> {
    Ok(Json(
        state
            .sso
            .configure(
                vendor.vendor_id,
                vendor.user_id,
                body.required,
                body.domain.as_deref(),
            )
            .await
            .map_err(ApiError)?,
    ))
}

#[derive(Deserialize)]
struct BindBody {
    subject: String,
}

async fn bind(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Path(id): Path<Uuid>,
    Json(body): Json<BindBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let role = state
        .vendor_team
        .role_of(vendor.vendor_id, vendor.user_id)
        .await
        .map_err(ApiError)?;
    if !role.can_manage_team() {
        return Err(ApiError(kernel::AppError::forbidden(
            "only an owner or an admin can bind an identity",
        )));
    }

    state
        .sso
        .bind_subject(vendor.vendor_id, id, &body.subject)
        .await
        .map_err(ApiError)?;

    Ok(Json(serde_json::json!({ "bound": id })))
}
