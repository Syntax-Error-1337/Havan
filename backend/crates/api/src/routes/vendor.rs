use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use domain::catalog::Product;
use domain::serials::SerialRepo;
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
        .route(
            "/vendor/products/{id}/serials",
            get(serials).post(add_serials),
        )
        // The fleet: what the vendor shipped and what it is doing. Read-only, and
        // deliberately without customer identity — see `domain::fleet`.
        .route("/vendor/devices", get(fleet))
        .route("/vendor/analytics", get(analytics))
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

#[derive(Deserialize)]
struct SerialsBody {
    serials: Vec<String>,
}

/// Upload a manufacturing manifest.
///
/// Idempotent by design: re-uploading a batch that is half-known adds the
/// missing half and says how many were already there. A vendor reconciling two
/// spreadsheets should not have to diff them by hand.
async fn add_serials(
    State(state): State<AppState>,
    vendor: VendorAuth,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    Json(body): Json<SerialsBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let serials = domain::serials::normalise(body.serials);
    if serials.is_empty() {
        return Err(ApiError(kernel::AppError::validation(
            "no usable serials in that list",
        )));
    }

    let (added, already) = state
        .serials
        .add(vendor.vendor_id, id, &serials)
        .await
        .map_err(ApiError)?;

    let summary = state
        .serials
        .summary(vendor.vendor_id, id)
        .await
        .map_err(ApiError)?;

    Ok(Json(serde_json::json!({
        "added": added,
        "already_listed": already,
        "summary": summary,
    })))
}

async fn serials(
    State(state): State<AppState>,
    vendor: VendorAuth,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> ApiResult<Json<domain::serials::SerialSummary>> {
    Ok(Json(
        state
            .serials
            .summary(vendor.vendor_id, id)
            .await
            .map_err(ApiError)?,
    ))
}

/// `GET /vendor/devices?product_key=&state=online|offline|bound|unbound&cursor=&limit=`
async fn fleet(
    State(state): State<AppState>,
    vendor: VendorAuth,
    axum::extract::Query(filter): axum::extract::Query<domain::fleet::FleetFilter>,
) -> ApiResult<Json<domain::fleet::FleetPage>> {
    domain::fleet::validate(&filter).map_err(ApiError)?;
    Ok(Json(
        state
            .fleet
            .page(vendor.vendor_id, filter)
            .await
            .map_err(ApiError)?,
    ))
}

/// `GET /vendor/analytics` — counts over the vendor's own devices. No estimates: every
/// number here is a `count(*)` over rows the vendor can already list one by one.
async fn analytics(
    State(state): State<AppState>,
    vendor: VendorAuth,
) -> ApiResult<Json<domain::fleet::FleetStats>> {
    Ok(Json(
        state
            .fleet
            .stats(vendor.vendor_id)
            .await
            .map_err(ApiError)?,
    ))
}
