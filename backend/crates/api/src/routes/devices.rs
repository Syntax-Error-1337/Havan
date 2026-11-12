use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use domain::devices::{Command, CommandOutcome, Device, PointValue};
use domain::ota::OtaRepo;
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::Auth;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/devices", get(list))
        .route("/devices/{id}", get(one).patch(update).delete(remove))
        .route("/devices/{id}/updates", get(updates))
        .route("/devices/{id}/commands", post(command))
        .route("/devices/{id}/commands/{command_id}", get(command_status))
}

#[derive(Deserialize)]
struct ListQuery {
    home_id: Uuid,
}

#[derive(Deserialize)]
struct UpdateBody {
    name: Option<String>,
    /// `Some(None)` clears the room; `None` leaves it alone.
    #[serde(default, deserialize_with = "double_option")]
    room_id: Option<Option<Uuid>>,
}

/// Distinguishes "field absent" from "field set to null" without a second DTO.
fn double_option<'de, D>(de: D) -> Result<Option<Option<Uuid>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Uuid>::deserialize(de).map(Some)
}

async fn list(
    State(state): State<AppState>,
    auth: Auth,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<Device>>> {
    Ok(Json(
        state
            .devices
            .list(q.home_id, auth.user.id)
            .await
            .map_err(ApiError)?,
    ))
}

/// The customer's view of their device's firmware: what it runs, what was
/// attempted, and how it went. No campaign, no rollout percentage, no vendor.
async fn updates(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<domain::ota::DeviceUpdate>> {
    // Ownership first: the OTA read itself runs on a system context.
    let device = state
        .devices
        .get(id, auth.user.id)
        .await
        .map_err(ApiError)?;

    let update = state
        .ota
        .device_update(id, device.fw_version.clone())
        .await
        .map_err(ApiError)?;

    Ok(Json(update))
}

async fn one(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Device>> {
    Ok(Json(
        state
            .devices
            .get(id, auth.user.id)
            .await
            .map_err(ApiError)?,
    ))
}

async fn update(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateBody>,
) -> ApiResult<Json<Device>> {
    let mut device = state
        .devices
        .get(id, auth.user.id)
        .await
        .map_err(ApiError)?;

    if let Some(name) = body.name {
        device = state
            .devices
            .rename(id, auth.user.id, &name)
            .await
            .map_err(ApiError)?;
    }
    if let Some(room) = body.room_id {
        device = state
            .devices
            .assign_room(id, auth.user.id, room)
            .await
            .map_err(ApiError)?;
    }
    Ok(Json(device))
}

async fn remove(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state
        .devices
        .unclaim(id, auth.user.id)
        .await
        .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct CommandBody {
    code: String,
    value: Option<serde_json::Value>,
    #[serde(default)]
    action: Option<String>,
}

/// `202 Accepted`, never `200`: the command is recorded and dispatched, not
/// applied. Only the device's ack settles it.
/// Send a freshly created command to its device.
///
/// Shared with scene activation on purpose: a scene must not grow a second way
/// for a device to be told something.
pub(crate) async fn publish_command(
    state: &AppState,
    device_id: Uuid,
    outcome: &CommandOutcome,
) -> Result<(), ApiError> {
    // Counted here, not in each caller: scenes and automations publish commands
    // through this function too, and a metric that misses half the traffic is worse
    // than none.
    kernel::metrics::command_dispatched();
    // A replay is already on the wire; publishing again would make the device
    // act twice on one press.
    let Some(publishable) = outcome.publish.as_ref() else {
        return Ok(());
    };

    let msg = protocol::envelope::CommandMsg {
        v: protocol::WIRE_VERSION,
        id: outcome.command.id,
        ts: kernel::now_s(),
        op: protocol::envelope::CommandOp::Set,
        dp: Some(publishable.dp.clone()),
        value: Some(to_dp_value(publishable.value.clone())),
        action: None,
    };

    state
        .mqtt
        .publish(
            &protocol::topics::cmd(
                &publishable.vendor_id.to_string(),
                &publishable.home_id.to_string(),
                &device_id.to_string(),
            ),
            serde_json::to_vec(&msg)
                .map_err(|e| ApiError(kernel::AppError::Internal(e.to_string())))?,
        )
        .await
        .map_err(ApiError)
}

async fn command(
    State(state): State<AppState>,
    auth: Auth,
    Path(id): Path<Uuid>,
    headers: axum::http::HeaderMap,
    Json(body): Json<CommandBody>,
) -> ApiResult<(StatusCode, Json<serde_json::Value>)> {
    if body.action.is_some() {
        return Err(ApiError(kernel::AppError::validation(
            "actions are not implemented yet; send a point value",
        )));
    }

    let value = body.value.map(to_point_value).transpose()?;
    let idempotency = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let outcome = state
        .devices
        .command(id, auth.user.id, &body.code, value, idempotency)
        .await
        .map_err(ApiError)?;

    publish_command(&state, id, &outcome).await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "command_id": outcome.command.id,
            "status": outcome.command.status,
        })),
    ))
}

async fn command_status(
    State(state): State<AppState>,
    auth: Auth,
    Path((_id, command_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<Command>> {
    Ok(Json(
        state
            .devices
            .command_status(command_id, auth.user.id)
            .await
            .map_err(ApiError)?,
    ))
}

fn to_point_value(v: serde_json::Value) -> ApiResult<PointValue> {
    match v {
        serde_json::Value::Bool(b) => Ok(PointValue::Bool(b)),
        serde_json::Value::Number(n) => n
            .as_f64()
            .map(PointValue::Num)
            .ok_or_else(|| ApiError(kernel::AppError::validation("value must be a number"))),
        serde_json::Value::String(s) => Ok(PointValue::Text(s)),
        _ => Err(ApiError(kernel::AppError::validation(
            "value must be a boolean, number or string",
        ))),
    }
}

/// The wire type is owned by `protocol`; the domain type is not, so the crossing
/// happens here and nowhere else.
fn to_dp_value(v: PointValue) -> protocol::envelope::DpValue {
    match v {
        PointValue::Bool(b) => protocol::envelope::DpValue::Bool(b),
        PointValue::Num(n) => {
            if n.fract() == 0.0 && n.abs() < i64::MAX as f64 {
                protocol::envelope::DpValue::Int(n as i64)
            } else {
                protocol::envelope::DpValue::Float(n)
            }
        }
        PointValue::Text(s) => protocol::envelope::DpValue::Text(s),
    }
}
