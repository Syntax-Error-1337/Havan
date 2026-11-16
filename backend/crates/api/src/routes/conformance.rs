use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use infra::repos::conformance::{ConformanceMaterial, DeviceSnapshot, PgConformance};
use kernel::{AppError, ErrorCode};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::ops::require_ops;
use crate::app::AppState;
use crate::error::{ApiError, ApiResult};
use crate::limits::Peer;

pub fn router() -> Router<AppState> {
    Router::new()
        // The vendor's door is not here: a conformance run is the operator's, so every path is
        // under /ops and every handler goes through require_ops.
        .route("/ops/conformance/sessions", post(start))
        .route("/ops/conformance/checklist", get(checklist))
        .route(
            "/ops/conformance/devices/{id}/commands",
            post(issue_command),
        )
        .route("/ops/devices/{id}/state", get(device_state))
}

#[derive(Deserialize)]
struct StartBody {
    vendor_id: Uuid,
    product_id: Uuid,
}

/// `POST /v1/ops/conformance/sessions`
///
/// Hands back everything needed to act like a device: the product key, a serial the platform
/// expects, the secret that serial will present, and a pairing ticket a customer would otherwise
/// tap in the app. The ticket is minted here rather than by the vendor because minting one is a
/// customer's act — this is the operator running the test on their behalf.
async fn start(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    peer: Peer,
    Json(body): Json<StartBody>,
) -> ApiResult<Json<ConformanceMaterial>> {
    require_ops(&state, &headers, &peer)?;

    let repo = PgConformance::new(state.pg.clone());
    let mut material = repo.prepare(body.vendor_id, body.product_id).await?;

    material.ticket = state
        .pairing
        .mint(material.home_id, material.user_id, kernel::new_id())
        .await?;

    Ok(Json(material))
}

/// `GET /v1/ops/devices/{id}/state`
///
/// The platform's own view of a device: whether it is online, what it last reported per capability,
/// and the last few commands with their outcome. A conformance run checks itself against this
/// rather than against a log line.
async fn device_state(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    peer: Peer,
    Path(id): Path<Uuid>,
    axum::extract::Query(q): axum::extract::Query<SnapshotQuery>,
) -> ApiResult<Json<DeviceSnapshot>> {
    require_ops(&state, &headers, &peer)?;

    let vendor_id = q.vendor_id.ok_or_else(|| {
        ApiError::from(AppError::code(
            ErrorCode::BadRequest,
            "vendor_id is required: a device is only visible inside its vendor",
        ))
    })?;

    let repo = PgConformance::new(state.pg.clone());
    Ok(Json(repo.snapshot(vendor_id, id).await?))
}

#[derive(Deserialize)]
struct CommandBody {
    code: String,
    value: Option<Value>,
}

/// `POST /v1/ops/conformance/devices/{id}/commands`
///
/// Issue a command to a device in the lab, the same way the app would — through the same creation
/// and dispatch path, so a passing run means the customer's path works. Refused for any device
/// outside the lab: an operator token must not be able to reach into somebody's home.
async fn issue_command(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    peer: Peer,
    Path(id): Path<Uuid>,
    axum::extract::Query(q): axum::extract::Query<SnapshotQuery>,
    Json(body): Json<CommandBody>,
) -> ApiResult<Json<Value>> {
    require_ops(&state, &headers, &peer)?;

    let vendor_id = q.vendor_id.ok_or_else(|| {
        ApiError::from(AppError::code(
            ErrorCode::BadRequest,
            "vendor_id is required",
        ))
    })?;

    let repo = PgConformance::new(state.pg.clone());
    let owner = repo.lab_owner(vendor_id, id).await?;

    let value = body.value.map(super::devices::to_point_value).transpose()?;

    let outcome = state
        .devices
        .command(id, owner, &body.code, value, None)
        .await
        .map_err(ApiError)?;

    super::devices::publish_command(&state, id, &outcome).await?;

    Ok(Json(json!({
        "command_id": outcome.command.id,
        "status": outcome.command.status,
    })))
}

#[derive(Deserialize)]
struct SnapshotQuery {
    vendor_id: Option<Uuid>,
}

/// `GET /v1/ops/conformance/checklist`
///
/// What the suite will check, so a vendor's script can print it before it runs. Static data, no
/// query: the checklist is the contract, and a script that invents its own is checking something
/// else.
async fn checklist(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    peer: Peer,
) -> ApiResult<Json<Value>> {
    require_ops(&state, &headers, &peer)?;
    Ok(Json(json!([
        "claim: a serial in the manifest becomes a device in the customer's home",
        "connect: the device authenticates with the secret it presented",
        "shadow: a reported value reaches the platform and is visible per capability",
        "command: an operator-issued command is delivered and acknowledged",
        "refusal: a serial outside the manifest is handled the way the product declares — refused, or accepted and charged",
        "reconnect: the device returns after a dropped connection"
    ])))
}
