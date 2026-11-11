use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use domain::scenes::{ActionFailure, Activation, Scene, SceneAction, SceneRepo};
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::Auth;
use crate::error::{ApiError, ApiResult};
use crate::routes::devices::publish_command;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/homes/{id}/scenes", get(list).post(create))
        .route("/scenes/{scene_id}", delete(remove))
        .route("/scenes/{scene_id}/activate", post(activate))
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
    #[serde(default)]
    icon: Option<String>,
    actions: Vec<SceneAction>,
}

async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Path(home_id): Path<Uuid>,
    Json(body): Json<CreateBody>,
) -> ApiResult<(StatusCode, Json<Scene>)> {
    if body.name.trim().is_empty() {
        return Err(ApiError(kernel::AppError::validation(
            "give the scene a name",
        )));
    }

    let home = state
        .homes
        .require(home_id, auth.user.id)
        .await
        .map_err(ApiError)?;

    // Any member can run a scene, but creating one changes the home for
    // everyone in it — the same rule as automations.
    if !home.role.can_manage() {
        return Err(ApiError(kernel::AppError::forbidden(
            "only an admin can create a scene",
        )));
    }

    let scene = Scene {
        id: kernel::new_id(),
        home_id,
        name: body.name.trim().to_string(),
        icon: body.icon,
        actions: body.actions,
        created_at: kernel::now(),
    };

    state
        .scenes
        .create(home_id, auth.user.id, &scene)
        .await
        .map_err(ApiError)?;

    Ok((StatusCode::CREATED, Json(scene)))
}

async fn list(
    State(state): State<AppState>,
    auth: Auth,
    Path(home_id): Path<Uuid>,
) -> ApiResult<Json<Vec<Scene>>> {
    state
        .homes
        .require(home_id, auth.user.id)
        .await
        .map_err(ApiError)?;

    Ok(Json(
        state
            .scenes
            .list(home_id, auth.user.id)
            .await
            .map_err(ApiError)?,
    ))
}

async fn remove(
    State(state): State<AppState>,
    auth: Auth,
    Path(scene_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state
        .scenes
        .delete(scene_id, auth.user.id)
        .await
        .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Press the button.
///
/// Every action goes through the same path a manual command does — one row in
/// the command log, one publish — so a scene cannot invent a second way for a
/// device to be told something. Failures are collected, not thrown: one dead
/// device must not stop "good night" from turning off the other nine.
async fn activate(
    State(state): State<AppState>,
    auth: Auth,
    Path(scene_id): Path<Uuid>,
) -> ApiResult<Json<Activation>> {
    let scene = state
        .scenes
        .get(scene_id, auth.user.id)
        .await
        .map_err(ApiError)?
        .ok_or_else(|| ApiError(kernel::AppError::not_found("scene")))?;

    let mut dispatched = 0usize;
    let mut failed = Vec::new();

    for (index, action) in scene.actions.iter().enumerate() {
        let SceneAction::SetDevice {
            device_id,
            code,
            value,
        } = action;

        let point_value = match to_point_value(value) {
            Ok(v) => v,
            Err(e) => {
                failed.push(ActionFailure {
                    index,
                    device_id: *device_id,
                    code: code.clone(),
                    message: e.0.to_string(),
                });
                continue;
            }
        };

        match state
            .devices
            .command(*device_id, auth.user.id, code, Some(point_value), None)
            .await
        {
            Ok(outcome) => {
                if let Err(e) = publish_command(&state, *device_id, &outcome).await {
                    failed.push(ActionFailure {
                        index,
                        device_id: *device_id,
                        code: code.clone(),
                        message: e.0.to_string(),
                    });
                    continue;
                }
                dispatched += 1;
            }
            Err(e) => failed.push(ActionFailure {
                index,
                device_id: *device_id,
                code: code.clone(),
                message: e.to_string(),
            }),
        }
    }

    Ok(Json(Activation {
        scene_id,
        dispatched,
        failed,
    }))
}

/// A scene stores JSON because it is data; the command path wants a typed value.
fn to_point_value(value: &serde_json::Value) -> Result<domain::devices::PointValue, ApiError> {
    match value {
        serde_json::Value::Bool(b) => Ok(domain::devices::PointValue::Bool(*b)),
        serde_json::Value::Number(n) => n
            .as_f64()
            .map(domain::devices::PointValue::Num)
            .ok_or_else(|| ApiError(kernel::AppError::validation("that number is out of range"))),
        serde_json::Value::String(s) => Ok(domain::devices::PointValue::Text(s.clone())),
        _ => Err(ApiError(kernel::AppError::validation(
            "a scene action sets a boolean, number or string",
        ))),
    }
}
