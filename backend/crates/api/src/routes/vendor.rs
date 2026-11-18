use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
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
        .route("/vendor/audit.csv", get(audit_csv))
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
    // The same two budgets as the customer sign-in: one per account, one per source.
    let source = format!("rl:vendor-login-source:{}", peer.key());
    crate::limits::check(&state, &key, crate::limits::LOGIN).await?;
    crate::limits::check(&state, &source, crate::limits::SOURCE).await?;

    let vendor_id = state.identity.resolve_vendor(&body.vendor).await?;

    match state
        .identity
        .login_vendor(vendor_id, &body.email, &body.password, body.totp.as_deref())
        .await
    {
        Ok(tokens) => Ok(Json(tokens)),
        Err(e) => {
            crate::limits::record(&state, &key, crate::limits::LOGIN).await;
            crate::limits::record(&state, &source, crate::limits::SOURCE).await;
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

    let url = body.url.trim().to_string();
    let (id, secret) = state
        .webhooks
        .create(vendor.vendor_id, &url, body.events)
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
            .with_request(&vendor.request_id)
            // The destination, not just the id: "which webhook was that" is the first question anybody asks of this
            // row, and an audit entry that cannot answer it is a pointer to a table the reader may no longer have.
            .detail(serde_json::json!({ "url": url })),
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

/// `GET /v1/vendor/audit.csv` — the trail as a file, with the same filters as the page.
///
/// An export is what an auditor actually needs: a list on a screen is not evidence you can attach to a report. The rows
/// are the same rows the page reads, written in the same order, so the file and the screen cannot disagree.
async fn audit_csv(
    State(state): State<AppState>,
    vendor: VendorAuth,
    axum::extract::Query(q): axum::extract::Query<AuditQuery>,
) -> ApiResult<Response> {
    // Ten thousand rows is a range nobody reads and every spreadsheet opens. The response says when it stopped rather
    // than handing back a file that quietly ends.
    const MAX_ROWS: i64 = 10_000;

    let (rows, truncated) = state
        .audit
        .export(
            vendor.vendor_id,
            &domain::audit::AuditFilter {
                action: q.action,
                actor: q.actor,
            },
            MAX_ROWS,
        )
        .await
        .map_err(ApiError)?;

    let body = to_csv(&rows);
    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "text/csv; charset=utf-8".to_string(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"haven-audit.csv\"".to_string(),
            ),
            // Readable by the portal so it can say "this file stops at 10,000" instead of implying it has everything.
            (
                axum::http::HeaderName::from_static("x-audit-truncated"),
                truncated.to_string(),
            ),
        ],
        body,
    )
        .into_response())
}

/// The CSV, with every field quoted and inner quotes doubled.
///
/// Quoting everything is not laziness: an audit row carries a detail blob, and a comma or a newline in one of them is
/// ordinary. A file that shifts a column because somebody's name had a comma is a file that lies about what happened.
fn to_csv(rows: &[domain::audit::AuditRow]) -> String {
    fn field(value: &str) -> String {
        format!("\"{}\"", value.replace('"', "\"\""))
    }

    let mut out =
        String::from("when,actor_kind,actor,action,target_kind,target_id,ip,request_id,detail\n");
    for row in rows {
        let detail = serde_json::to_string(&row.detail).unwrap_or_default();
        out.push_str(
            &[
                field(
                    &row.ts
                        .format(&time::format_description::well_known::Rfc3339)
                        .unwrap_or_default(),
                ),
                field(&row.actor_kind),
                field(row.actor_email.as_deref().unwrap_or("")),
                field(&row.action),
                field(&row.target_kind),
                field(&row.target_id.map(|id| id.to_string()).unwrap_or_default()),
                field(row.ip.as_deref().unwrap_or("")),
                field(row.request_id.as_deref().unwrap_or("")),
                field(&detail),
            ]
            .join(","),
        );
        out.push('\n');
    }
    out
}

#[derive(serde::Deserialize)]
struct AuditQuery {
    limit: Option<i64>,
    cursor: Option<Uuid>,
    /// Exact action, e.g. `api_key.revoked`.
    action: Option<String>,
    /// Part of the actor's address.
    actor: Option<String>,
}

async fn audit(
    State(state): State<AppState>,
    vendor: VendorAuth,
    axum::extract::Query(q): axum::extract::Query<AuditQuery>,
) -> ApiResult<Json<domain::audit::AuditPage>> {
    Ok(Json(
        state
            .audit
            .page(
                vendor.vendor_id,
                q.cursor,
                q.limit,
                &domain::audit::AuditFilter {
                    action: q.action,
                    actor: q.actor,
                },
            )
            .await
            .map_err(ApiError)?,
    ))
}
