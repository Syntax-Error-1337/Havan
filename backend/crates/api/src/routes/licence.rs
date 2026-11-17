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
use axum::response::IntoResponse;
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
        .route("/vendor/invoices", get(my_invoices))
        .route("/vendor/invoices/{period}", get(my_invoice))
        .route("/ops/invoices", get(invoices).post(issue))
        .route("/ops/invoices.csv", get(invoice_csv))
}

#[derive(serde::Deserialize)]
struct PeriodQuery {
    period: Option<String>,
    vendor_id: Option<Uuid>,
}

/// The current month, in UTC, so a caller that does not name a period gets the one they are
/// living in rather than an error.
fn period_or_now(period: Option<String>) -> Result<String, ApiError> {
    match period {
        Some(p) => {
            domain::licence::period_bounds(&p).map_err(ApiError)?;
            Ok(p)
        }
        None => Ok(domain::licence::period_of(time::OffsetDateTime::now_utc())),
    }
}

/// `GET /v1/vendor/invoices?period=YYYY-MM` — this month as a draft, or the document if issued.
async fn my_invoices(
    State(state): State<AppState>,
    vendor: VendorAuth,
    Query(q): Query<PeriodQuery>,
) -> ApiResult<Json<domain::licence::Invoice>> {
    let period = period_or_now(q.period)?;
    Ok(Json(
        state
            .licences
            .invoice_preview(vendor.vendor_id, &period)
            .await
            .map_err(ApiError)?,
    ))
}

/// `GET /v1/vendor/invoices/{period}` — the same thing, addressed by month, because "the invoice
/// for March" is how a finance team asks for it.
async fn my_invoice(
    State(state): State<AppState>,
    vendor: VendorAuth,
    axum::extract::Path(period): axum::extract::Path<String>,
) -> ApiResult<Json<domain::licence::Invoice>> {
    let period = period_or_now(Some(period))?;
    Ok(Json(
        state
            .licences
            .invoice_preview(vendor.vendor_id, &period)
            .await
            .map_err(ApiError)?,
    ))
}

/// `GET /v1/ops/invoices?vendor_id=&period=` — one month, live or frozen.
async fn invoices(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
    Query(q): Query<PeriodQuery>,
) -> ApiResult<Json<domain::licence::Invoice>> {
    super::ops::require_ops(&state, &headers, &peer)?;

    let vendor_id = q.vendor_id.ok_or_else(|| {
        ApiError::from(kernel::AppError::code(
            kernel::ErrorCode::BadRequest,
            "vendor_id is required",
        ))
    })?;
    let period = period_or_now(q.period)?;

    Ok(Json(
        state
            .licences
            .invoice_preview(vendor_id, &period)
            .await
            .map_err(ApiError)?,
    ))
}

#[derive(serde::Deserialize)]
struct IssueBody {
    vendor_id: Uuid,
    period: String,
}

/// `POST /v1/ops/invoices` — freeze a month. Repeating it returns the first document.
async fn issue(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
    Json(body): Json<IssueBody>,
) -> ApiResult<Json<domain::licence::Invoice>> {
    super::ops::require_ops(&state, &headers, &peer)?;

    Ok(Json(
        state
            .licences
            .invoice_issue(body.vendor_id, &body.period, None)
            .await
            .map_err(ApiError)?,
    ))
}

/// `GET /v1/ops/invoices.csv?vendor_id=&period=` — the export a finance system imports.
///
/// CSV because the receiving end is a spreadsheet or a ledger, and both of those want lines, not
/// nested JSON. The totals go in a final row so a reader can check their import against the
/// document without recomputing it.
async fn invoice_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: Peer,
    Query(q): Query<PeriodQuery>,
) -> ApiResult<axum::response::Response> {
    super::ops::require_ops(&state, &headers, &peer)?;

    let vendor_id = q.vendor_id.ok_or_else(|| {
        ApiError::from(kernel::AppError::code(
            kernel::ErrorCode::BadRequest,
            "vendor_id is required",
        ))
    })?;
    let period = period_or_now(q.period)?;
    let invoice = state
        .licences
        .invoice_preview(vendor_id, &period)
        .await
        .map_err(ApiError)?;

    let mut out = String::from("period,serial,product_key,unit_price_cents,activated_at\n");
    for line in &invoice.lines {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            invoice.period,
            csv_field(&line.serial),
            csv_field(&line.product_key),
            line.unit_price_cents,
            line.activated_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        ));
    }
    out.push_str(&format!(
        "{},TOTAL,,{},{}\n",
        invoice.period, invoice.amount_cents, invoice.currency
    ));

    // A name, because the file is downloaded and then emailed to somebody in finance: an export
    // called `download` in a folder of `download (3)`s is a support ticket waiting to happen.
    let filename = format!(
        "haven-invoices-{}-{}.csv",
        &vendor_id.to_string()[..8],
        invoice.period
    );

    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "text/csv; charset=utf-8".to_string(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        out,
    )
        .into_response())
}

/// A serial is a vendor's string, so it is quoted unless it is obviously plain. Numbers written as
/// text get an apostrophe so a spreadsheet does not turn a serial of 20 digits into a float.
fn csv_field(value: &str) -> String {
    let needs_quotes = value.contains([',', '"', '\n', '\r']);
    // Only a string that *is* a number: an allowlist of numeric characters would catch a serial
    // like `E2E-1794452749` because it contains an `E`, and mangle it in somebody's spreadsheet.
    let looks_numeric = !value.is_empty() && value.parse::<f64>().is_ok();

    if needs_quotes {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else if looks_numeric {
        format!("'{value}")
    } else {
        value.to_string()
    }
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
