use axum::Json;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use domain::identity::{RegisterInput, Tokens};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::auth::Auth;
use crate::error::{ApiError, ApiResult};
use crate::limits::{self, Peer};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
        .route("/me", get(me))
}

#[derive(Deserialize)]
struct RegisterBody {
    email: String,
    password: String,
    #[serde(default)]
    display_name: String,
}

#[derive(Deserialize)]
struct LoginBody {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct RefreshBody {
    refresh: String,
}

#[derive(Serialize)]
struct MeBody {
    id: uuid::Uuid,
    email: Option<String>,
    display_name: String,
    locale: String,
}

async fn register(
    State(state): State<AppState>,
    peer: Peer,
    Json(body): Json<RegisterBody>,
) -> ApiResult<(StatusCode, Json<Tokens>)> {
    let key = format!("rl:register:{}", peer.key());
    limits::check(&state, &key, limits::REGISTER).await?;

    let result = state
        .identity
        .register(RegisterInput {
            email: body.email,
            password: body.password,
            display_name: body.display_name,
        })
        .await;

    match result {
        Ok(tokens) => Ok((StatusCode::CREATED, Json(tokens))),
        Err(e) => {
            limits::record(&state, &key, limits::REGISTER).await;
            Err(ApiError(e))
        }
    }
}

async fn login(
    State(state): State<AppState>,
    peer: Peer,
    Json(body): Json<LoginBody>,
) -> ApiResult<Json<Tokens>> {
    // Keyed by source *and* account: a person who mistypes their own password is one
    // person, while somebody working through a list of addresses is another, and they
    // must not share a budget.
    let key = format!("rl:login:{}:{}", peer.key(), body.email.to_lowercase());
    limits::check(&state, &key, limits::LOGIN).await?;

    match state.identity.login(&body.email, &body.password).await {
        Ok(tokens) => Ok(Json(tokens)),
        Err(e) => {
            // Only failures are counted, and only after the credentials were proven
            // wrong — see `limits`.
            limits::record(&state, &key, limits::LOGIN).await;
            Err(ApiError(e))
        }
    }
}

async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshBody>,
) -> ApiResult<Json<Tokens>> {
    Ok(Json(
        state
            .identity
            .refresh(&body.refresh)
            .await
            .map_err(ApiError)?,
    ))
}

async fn logout(
    State(state): State<AppState>,
    Json(body): Json<RefreshBody>,
) -> ApiResult<StatusCode> {
    state
        .identity
        .logout(&body.refresh)
        .await
        .map_err(ApiError)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn me(auth: Auth) -> ApiResult<Json<MeBody>> {
    Ok(Json(MeBody {
        id: auth.user.id,
        email: auth.user.email,
        display_name: auth.user.display_name,
        locale: auth.user.locale,
    }))
}
