use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use domain::catalog::Product;
use protocol::thing_model::ThingModel;
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::{Auth, VendorAuth};
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/vendor/auth/login", post(login))
        .route("/vendor/products", get(list).post(create))
        .route("/vendor/products/{id}", get(one).patch(update))
        .route("/vendor/products/{id}/validate", post(validate))
        .route("/vendor/products/{id}/publish", post(publish))
}

#[derive(Deserialize)]
struct LoginBody {
    vendor_id: Uuid,
    email: String,
    password: String,
}

/// The portal's door. Separate from `/auth/login`: different table, different
/// tenancy, and a customer account must never be able to become a vendor one.
async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginBody>,
) -> ApiResult<Json<domain::identity::Tokens>> {
    let tokens = state
        .identity
        .login_vendor(body.vendor_id, &body.email, &body.password)
        .await
        .map_err(ApiError)?;
    Ok(Json(tokens))
}

async fn list(State(state): State<AppState>, vendor: VendorAuth) -> ApiResult<Json<Vec<Product>>> {
    Ok(Json(
        state
            .catalog
            .list(vendor.vendor_id)
            .await
            .map_err(ApiError)?,
    ))
}

#[derive(Deserialize)]
struct CreateBody {
    thing_model: ThingModel,
    #[serde(default)]
    publish: bool,
}

async fn create(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Json(body): Json<CreateBody>,
) -> ApiResult<(StatusCode, Json<Product>)> {
    let product = state
        .catalog
        .create(vendor.vendor_id, body.thing_model, body.publish)
        .await
        .map_err(ApiError)?;
    Ok((StatusCode::CREATED, Json(product)))
}

async fn one(
    State(state): State<AppState>,
    vendor: VendorAuth,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> ApiResult<Json<Product>> {
    Ok(Json(
        state
            .catalog
            .get(vendor.vendor_id, id)
            .await
            .map_err(ApiError)?,
    ))
}

async fn update(
    State(state): State<AppState>,
    vendor: VendorAuth,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    Json(body): Json<CreateBody>,
) -> ApiResult<Json<Product>> {
    Ok(Json(
        state
            .catalog
            .update(vendor.vendor_id, id, body.thing_model)
            .await
            .map_err(ApiError)?,
    ))
}

/// Returns violations rather than failing: the portal renders them beside the
/// fields that caused them, and a 200 with an empty list means "ready".
async fn validate(
    State(state): State<AppState>,
    vendor: VendorAuth,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let violations = state
        .catalog
        .validate(vendor.vendor_id, id)
        .await
        .map_err(ApiError)?;
    Ok(Json(serde_json::json!({
        "valid": violations.is_empty(),
        "violations": violations,
    })))
}

async fn publish(
    State(state): State<AppState>,
    vendor: VendorAuth,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> ApiResult<Json<Product>> {
    Ok(Json(
        state
            .catalog
            .publish(vendor.vendor_id, id)
            .await
            .map_err(ApiError)?,
    ))
}

/// Kept so `Auth` stays referenced in this module's imports for the shared
/// session type; the vendor surface deliberately does not accept a customer
/// token.
#[allow(dead_code)]
fn _customer_tokens_are_not_vendor_tokens(_: Auth) {}
