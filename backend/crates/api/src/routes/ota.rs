use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use domain::ota::{Campaign, CampaignStatus, Firmware, OtaRepo};
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::VendorAuth;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/vendor/products/{product_id}/firmware",
            get(list_firmware).post(add_firmware),
        )
        .route("/vendor/campaigns", get(list).post(create))
        .route("/vendor/campaigns/{id}", get(one))
        .route("/vendor/campaigns/{id}/start", post(start))
}

#[derive(Deserialize)]
struct FirmwareBody {
    version: String,
    size: i64,
    sha256: String,
    ed25519_sig: String,
    object_key: String,
}

/// Register an image the vendor has already uploaded and signed.
///
/// The binary does not travel through the API: a 2 MB image behind a JSON body
/// would be buffered in the app process, and the signature has to be produced
/// somewhere the vendor controls anyway.
async fn add_firmware(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Path(product_id): Path<Uuid>,
    Json(body): Json<FirmwareBody>,
) -> ApiResult<(StatusCode, Json<Firmware>)> {
    if body.version.trim().is_empty() {
        return Err(ApiError(kernel::AppError::validation(
            "a version is required",
        )));
    }
    if body.size <= 0 {
        return Err(ApiError(kernel::AppError::validation(
            "size must be positive",
        )));
    }
    if body.sha256.len() != 64 || !body.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError(kernel::AppError::validation(
            "sha256 must be 64 hex characters",
        )));
    }

    let firmware = Firmware {
        id: kernel::new_id(),
        product_id,
        version: body.version.trim().to_string(),
        size: body.size,
        sha256: body.sha256.to_lowercase(),
        ed25519_sig: body.ed25519_sig,
        object_key: body.object_key,
        created_at: kernel::now(),
    };

    state
        .ota
        .add_firmware(vendor.vendor_id, product_id, &firmware)
        .await
        .map_err(ApiError)?;

    Ok((StatusCode::CREATED, Json(firmware)))
}

async fn list_firmware(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Path(product_id): Path<Uuid>,
) -> ApiResult<Json<Vec<Firmware>>> {
    Ok(Json(
        state
            .ota
            .firmware_for_product(vendor.vendor_id, product_id)
            .await
            .map_err(ApiError)?,
    ))
}

#[derive(Deserialize)]
struct CampaignBody {
    product_id: Uuid,
    firmware_id: Uuid,
    /// 0–100. A campaign defaults to nothing rather than everything: the safe
    /// mistake is one that updates too few devices.
    #[serde(default)]
    rollout_pct: Option<i32>,
}

async fn create(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Json(body): Json<CampaignBody>,
) -> ApiResult<(StatusCode, Json<Campaign>)> {
    let pct = body.rollout_pct.unwrap_or(0);
    if !(0..=100).contains(&pct) {
        return Err(ApiError(kernel::AppError::validation(
            "rollout_pct must be between 0 and 100",
        )));
    }

    let campaign = Campaign {
        id: kernel::new_id(),
        vendor_id: vendor.vendor_id,
        product_id: body.product_id,
        firmware_id: body.firmware_id,
        rollout_pct: pct,
        status: CampaignStatus::Draft,
        created_at: kernel::now(),
        started_at: None,
    };

    state
        .ota
        .create_campaign(vendor.vendor_id, &campaign)
        .await
        .map_err(ApiError)?;

    Ok((StatusCode::CREATED, Json(campaign)))
}

/// Every campaign with its progress, newest first.
///
/// One request for the whole table: a portal that fetches progress per row turns
/// a 40-campaign catalogue into 41 round trips.
async fn list(
    State(state): State<AppState>,
    vendor: VendorAuth,
) -> ApiResult<Json<Vec<serde_json::Value>>> {
    let campaigns = state
        .ota
        .campaigns(vendor.vendor_id)
        .await
        .map_err(ApiError)?;

    let mut out = Vec::with_capacity(campaigns.len());
    for campaign in campaigns {
        let progress = state
            .ota
            .progress(vendor.vendor_id, campaign.id)
            .await
            .map_err(ApiError)?;
        out.push(serde_json::json!({
            "campaign": campaign,
            "progress": progress,
            "complete": progress.is_complete(),
        }));
    }

    Ok(Json(out))
}

async fn one(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let campaign = state
        .ota
        .campaign(vendor.vendor_id, id)
        .await
        .map_err(ApiError)?
        .ok_or_else(|| ApiError(kernel::AppError::not_found("campaign")))?;

    // Counts, not just a status: "running" says nothing about whether ninety
    // devices failed ten minutes ago.
    let progress = state
        .ota
        .progress(vendor.vendor_id, id)
        .await
        .map_err(ApiError)?;

    Ok(Json(serde_json::json!({
        "campaign": campaign,
        "progress": progress,
        "complete": progress.is_complete(),
    })))
}

/// Freeze the wave and queue the jobs. Returns how many devices were queued —
/// a campaign that quietly matched nobody is the failure mode worth surfacing.
async fn start(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let queued = state
        .ota
        .start_campaign(vendor.vendor_id, id)
        .await
        .map_err(ApiError)?;

    Ok(Json(serde_json::json!({ "queued": queued })))
}
