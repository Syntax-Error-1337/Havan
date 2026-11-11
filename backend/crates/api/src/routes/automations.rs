use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use domain::automation::{Automation, AutomationDef};
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::Auth;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/homes/{id}/automations", get(list).post(create))
        .route("/automations/{automation_id}", delete(remove))
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
    definition: AutomationDef,
}

async fn list(
    State(state): State<AppState>,
    auth: Auth,
    Path(home_id): Path<Uuid>,
) -> ApiResult<Json<Vec<Automation>>> {
    state
        .homes
        .require(home_id, auth.user.id)
        .await
        .map_err(ApiError)?;

    Ok(Json(
        state.automations.list(home_id).await.map_err(ApiError)?,
    ))
}

async fn create(
    State(state): State<AppState>,
    auth: Auth,
    Path(home_id): Path<Uuid>,
    Json(body): Json<CreateBody>,
) -> ApiResult<(StatusCode, Json<Automation>)> {
    let home = state
        .homes
        .require(home_id, auth.user.id)
        .await
        .map_err(ApiError)?;
    if !home.role.can_manage() {
        return Err(ApiError(kernel::AppError::forbidden(
            "only an admin can create an automation",
        )));
    }

    let automation = state
        .automations
        .create(home_id, &body.name, body.definition)
        .await
        .map_err(ApiError)?;

    Ok((StatusCode::CREATED, Json(automation)))
}

/// Deleting needs the automation's home, which the caller proves by being a
/// member of it — a bare automation id is not a capability.
async fn remove(
    State(state): State<AppState>,
    auth: Auth,
    Path(automation_id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    let home_id = state
        .automations
        .home_of(automation_id)
        .await
        .map_err(ApiError)?
        .ok_or_else(|| ApiError(kernel::AppError::not_found("automation")))?;

    let home = state
        .homes
        .require(home_id, auth.user.id)
        .await
        .map_err(ApiError)?;
    if !home.role.can_manage() {
        return Err(ApiError(kernel::AppError::forbidden(
            "only an admin can delete an automation",
        )));
    }

    state
        .automations
        .delete(automation_id)
        .await
        .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}
