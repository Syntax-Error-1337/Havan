use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use domain::catalog::Product;
use domain::serials::SerialRepo;
use domain::webhook::WebhookRepo;
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
        .route(
            "/vendor/products/{id}",
            get(one).patch(update).delete(remove_product),
        )
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
        // What a vendor hands their support desk: allowlisted, never a redacted row.
        .route("/vendor/support-bundle", get(support_bundle))
        // Outbound events: what our systems tell theirs.
        .route("/vendor/webhooks", get(webhooks).post(subscribe_webhook))
        .route("/vendor/webhooks/{id}", delete(unsubscribe_webhook))
        // Who did what. Read-only, newest first, keyset-paginated.
        .route("/vendor/audit", get(audit))
}

#[derive(Deserialize)]
struct LoginBody {
    /// The workspace: its `slug` (`acme`) or its id, because older clients and scripts send the id and
    /// both should keep working. A person is asked for the name.
    #[serde(alias = "vendor_id")]
    vendor: String,
    email: String,
    password: String,
    /// Present when the account has confirmed a second factor. Absent is not an error in itself —
    /// accounts without one must keep working — the login path decides.
    #[serde(default)]
    totp: Option<String>,
}

/// The portal's door. Separate from `/auth/login`: different table, different
/// tenancy, and a customer account must never be able to become a vendor one.
async fn login(
    State(state): State<AppState>,
    peer: crate::limits::Peer,
    Json(body): Json<LoginBody>,
) -> ApiResult<Json<domain::identity::Tokens>> {
    let key = format!(
        "rl:vendor-login:{}:{}",
        peer.key(),
        body.email.to_lowercase()
    );
    crate::limits::check(&state, &key, crate::limits::LOGIN).await?;

    let vendor_id = state.identity.resolve_vendor(&body.vendor).await?;

    match state
        .identity
        .login_vendor(vendor_id, &body.email, &body.password, body.totp.as_deref())
        .await
    {
        Ok(tokens) => Ok(Json(tokens)),
        Err(e) => {
            crate::limits::record(&state, &key, crate::limits::LOGIN).await;
            Err(ApiError(e))
        }
    }
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
/// `DELETE /vendor/products/{id}` — a catalogue entry that nothing points at.
///
/// The two refusals are not politeness: devices in people's homes make the entry permanent, and
/// licence activations are the billing record that would cascade away with it.
async fn remove_product(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state
        .catalog
        .delete(vendor.vendor_id, id)
        .await
        .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}

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
    peer: crate::limits::Peer,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> ApiResult<Json<Product>> {
    let product = state
        .catalog
        .publish(vendor.vendor_id, id)
        .await
        .map_err(ApiError)?;

    state
        .audit
        .record(
            domain::audit::AuditEntry::new(
                vendor.vendor_id,
                domain::audit::ActorKind::User,
                "product.published",
                "product",
            )
            .by(vendor.user_id)
            .on(id)
            .with_request(&vendor.request_id)
            .from_ip(peer.ip())
            .detail(serde_json::json!({ "product_key": product.product_key })),
        )
        .await;

    Ok(Json(product))
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

async fn webhooks(
    State(state): State<AppState>,
    vendor: VendorAuth,
) -> ApiResult<Json<Vec<domain::webhook::WebhookSubscription>>> {
    Ok(Json(
        state
            .webhooks
            .list(vendor.vendor_id)
            .await
            .map_err(ApiError)?,
    ))
}

#[derive(Deserialize)]
struct WebhookBody {
    url: String,
    events: Vec<String>,
}

/// `POST /vendor/webhooks` — the secret comes back once, like an API key.
async fn subscribe_webhook(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Json(body): Json<WebhookBody>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.events.is_empty() {
        return Err(ApiError(kernel::AppError::validation(
            "choose at least one event",
        )));
    }
    if let Some(unknown) = body
        .events
        .iter()
        .find(|e| !domain::webhook::is_known_event(e))
    {
        return Err(ApiError(kernel::AppError::validation(format!(
            "unknown event: {unknown}"
        ))));
    }

    let (id, secret) = state
        .webhooks
        .create(vendor.vendor_id, body.url.trim(), body.events)
        .await
        .map_err(ApiError)?;

    state
        .audit
        .record(
            domain::audit::AuditEntry::new(
                vendor.vendor_id,
                domain::audit::ActorKind::User,
                "webhook.created",
                "vendor_webhook",
            )
            .by(vendor.user_id)
            .on(id)
            .with_request(&vendor.request_id),
        )
        .await;

    Ok(Json(serde_json::json!({
        "id": id,
        "secret": secret,
        "note": "shown once; verify deliveries with it and keep it out of logs"
    })))
}

async fn unsubscribe_webhook(
    State(state): State<AppState>,
    vendor: VendorAuth,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    state
        .webhooks
        .delete(vendor.vendor_id, id)
        .await
        .map_err(ApiError)?;
    Ok(Json(serde_json::json!({ "removed": id })))
}

/// `GET /vendor/support-bundle?device_id=` — an allowlisted bundle for a support ticket.
async fn support_bundle(
    State(state): State<AppState>,
    vendor: VendorAuth,
    axum::extract::Query(q): axum::extract::Query<SupportQuery>,
) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(
        state
            .support
            .bundle(vendor.vendor_id, q.device_id)
            .await
            .map_err(ApiError)?,
    ))
}

#[derive(serde::Deserialize)]
struct SupportQuery {
    device_id: Option<Uuid>,
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

#[derive(serde::Deserialize)]
struct AuditQuery {
    limit: Option<i64>,
    cursor: Option<Uuid>,
}

async fn audit(
    State(state): State<AppState>,
    vendor: VendorAuth,
    axum::extract::Query(q): axum::extract::Query<AuditQuery>,
) -> ApiResult<Json<domain::audit::AuditPage>> {
    Ok(Json(
        state
            .audit
            .page(vendor.vendor_id, q.cursor, q.limit)
            .await
            .map_err(ApiError)?,
    ))
}
