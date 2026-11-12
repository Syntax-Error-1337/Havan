use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use domain::automation::{Automation, AutomationDef, RunRecord};
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::Auth;
use crate::error::{ApiError, ApiResult};

#[derive(serde::Deserialize)]
struct HookQuery {
    token: String,
}

/// `POST /automations/{id}/hook?token=…` — the inbound edge of the automation system.
async fn webhook(
    State(state): State<AppState>,
    Path(automation_id): Path<Uuid>,
    axum::extract::Query(q): axum::extract::Query<HookQuery>,
) -> ApiResult<StatusCode> {
    let armed = state
        .automations
        .fire_webhook(automation_id, &q.token)
        .await
        .map_err(ApiError)?;

    if !armed {
        // Same answer for a bad token and a missing rule.
        return Err(ApiError(kernel::AppError::not_found("automation")));
    }
    Ok(StatusCode::ACCEPTED)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/homes/{id}/automations", get(list).post(create))
        .route("/automations/{automation_id}", delete(remove).patch(update))
        .route("/automations/{automation_id}/runs", get(runs))
        // No bearer token: the URL is the credential. The response is `202` because
        // the rule is *armed*, not run — the worker picks it up on its next tick,
        // which keeps every execution in one place instead of two.
        .route("/automations/{automation_id}/hook", post(webhook))
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

#[derive(Deserialize)]
struct UpdateBody {
    enabled: bool,
}

/// On or off. The home check is the same one `remove` makes: an automation id
/// is not a capability, membership of its home is.
async fn update(
    State(state): State<AppState>,
    auth: Auth,
    Path(automation_id): Path<Uuid>,
    Json(body): Json<UpdateBody>,
) -> ApiResult<Json<Automation>> {
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
            "only an admin can change an automation",
        )));
    }

    Ok(Json(
        state
            .automations
            .set_enabled(automation_id, body.enabled)
            .await
            .map_err(ApiError)?,
    ))
}

/// The last runs, newest first. `?limit=` is clamped server-side.
#[derive(Deserialize)]
struct RunsQuery {
    limit: Option<i64>,
}

async fn runs(
    State(state): State<AppState>,
    auth: Auth,
    Path(automation_id): Path<Uuid>,
    Query(q): Query<RunsQuery>,
) -> ApiResult<Json<Vec<RunRecord>>> {
    let home_id = state
        .automations
        .home_of(automation_id)
        .await
        .map_err(ApiError)?
        .ok_or_else(|| ApiError(kernel::AppError::not_found("automation")))?;

    state
        .homes
        .require(home_id, auth.user.id)
        .await
        .map_err(ApiError)?;

    Ok(Json(
        state
            .automations
            .runs(automation_id, q.limit.unwrap_or(20))
            .await
            .map_err(ApiError)?,
    ))
}
