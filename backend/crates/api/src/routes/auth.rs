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
        .route("/me/sessions", get(sessions))
        .route("/auth/logout-others", post(logout_others))
        .route("/auth/password", post(change_password))
        .route(
            "/me/totp",
            get(totp_state).post(start_totp).delete(disable_totp),
        )
        .route("/me/totp/confirm", post(confirm_totp))
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
    /// The second factor, when the account has one. Optional in the shape and required in the service for those
    /// accounts, so a phone that has never enrolled sends the same body it always did.
    #[serde(default)]
    code: Option<String>,
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
    limits::check(&state, &key, limits::REGISTER, "register").await?;

    // Kept for the invitation check below: the body is consumed by `register`.
    let email = body.email.clone();

    let result = state
        .identity
        .register(RegisterInput {
            email: body.email,
            password: body.password,
            display_name: body.display_name,
        })
        .await;

    match result {
        Ok(tokens) => {
            // Any home that invited this address while it had no account: join them now. Best effort — a registration
            // that succeeded must not fail because an invitation could not be accepted, and the person can still use the
            // link they were sent.
            if let Ok(claims) = state.identity.verify_access(&tokens.access) {
                match state.homes.accept_pending(claims.sub, &email).await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!(joined = n, "joined homes from pending invitations"),
                    Err(e) => tracing::warn!(error = ?e, "could not accept pending invitations"),
                }
            }
            Ok((StatusCode::CREATED, Json(tokens)))
        }
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
    // Two budgets: one that follows the account, and one that follows the source. A spray across many addresses never
    // fills the first; a shared router never fills the second.
    let source = format!("rl:login-source:{}", peer.key());
    limits::check(&state, &key, limits::LOGIN, "login").await?;
    limits::check(&state, &source, limits::SOURCE, "login-source").await?;

    match state
        .identity
        .login_with_code(&body.email, &body.password, body.code.as_deref())
        .await
    {
        Ok(tokens) => Ok(Json(tokens)),
        Err(e) => {
            // Only failures are counted, and only after the credentials were proven
            // wrong — see `limits`.
            limits::record(&state, &key, limits::LOGIN).await;
            limits::record(&state, &source, limits::SOURCE).await;
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

/// The devices this account is signed in on.
///
/// No device names and no locations: this service never stored them, and inventing "Chrome on macOS" from a user-agent
/// it does not keep would be a guess dressed as a fact. Started, last refreshed, expires on its own — enough to answer
/// "is that me?".
async fn sessions(State(state): State<AppState>, auth: Auth) -> ApiResult<Json<serde_json::Value>> {
    let (count, sessions) = state
        .identity
        .sessions(auth.user.id)
        .await
        .map_err(ApiError)?;

    // `count` is the true number; `sessions` is at most twenty of them, newest first. A screen that printed the length
    // of the list would under-report an account with hundreds of sign-ins.
    Ok(Json(serde_json::json!({
        "count": count,
        "sessions": sessions,
    })))
}

#[derive(Deserialize)]
struct PasswordBody {
    current: String,
    next: String,
    /// The caller's own refresh token, when it has one: the device that made the change keeps its session.
    #[serde(default)]
    refresh: Option<String>,
}

/// `POST /v1/auth/password` — change the password, with the old one as proof.
///
/// Every other session is revoked. A password change is the one moment somebody can act on "I think another person has
/// this", and leaving those sessions alive would make the new password theatre.
async fn change_password(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<PasswordBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let revoked = state
        .identity
        .change_password(
            auth.user.id,
            &body.current,
            &body.next,
            body.refresh.as_deref(),
        )
        .await
        .map_err(ApiError)?;

    Ok(Json(serde_json::json!({ "signed_out": revoked })))
}

#[derive(Deserialize)]
struct ConfirmBody {
    code: String,
}

/// `GET /v1/me/totp` — off, half-on, or on.
async fn totp_state(
    State(state): State<AppState>,
    user: Auth,
) -> ApiResult<Json<serde_json::Value>> {
    let state_word = state
        .identity
        .totp_state(user.user.id)
        .await
        .map_err(ApiError)?;
    Ok(Json(serde_json::json!({ "state": state_word })))
}

/// `POST /v1/me/totp` — begin an enrolment.
///
/// The seed is in this response and nowhere else it can be read: an endpoint that could hand a seed back later would be a
/// second factor the server can be asked to disable.
async fn start_totp(
    State(state): State<AppState>,
    user: Auth,
) -> ApiResult<Json<serde_json::Value>> {
    let started = state
        .identity
        .start_totp(
            user.user.id,
            user.user.email.as_deref().unwrap_or("your Haven account"),
        )
        .await
        .map_err(ApiError)?;

    Ok(Json(serde_json::json!({
        "secret": started.secret,
        "uri": started.uri,
    })))
}

/// `POST /v1/me/totp/confirm` — prove the authenticator works before it starts gating sign-ins.
async fn confirm_totp(
    State(state): State<AppState>,
    user: Auth,
    Json(body): Json<ConfirmBody>,
) -> ApiResult<Json<serde_json::Value>> {
    state
        .identity
        .confirm_totp(user.user.id, &body.code)
        .await
        .map_err(ApiError)?;

    tracing::info!(user = %user.user.id, "second factor confirmed");
    Ok(Json(serde_json::json!({ "state": "on" })))
}

/// `DELETE /v1/me/totp` — turn it off again.
async fn disable_totp(State(state): State<AppState>, user: Auth) -> ApiResult<StatusCode> {
    state
        .identity
        .disable_totp(user.user.id)
        .await
        .map_err(ApiError)?;

    tracing::info!(user = %user.user.id, "second factor turned off");
    Ok(StatusCode::NO_CONTENT)
}

/// Sign out everywhere except the device that asked.
async fn logout_others(
    State(state): State<AppState>,
    auth: Auth,
    Json(body): Json<RefreshBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let revoked = state
        .identity
        .logout_others(auth.user.id, &body.refresh)
        .await
        .map_err(ApiError)?;

    // `revoked: 0` is the normal answer for somebody with one phone — not an error.
    Ok(Json(serde_json::json!({ "revoked": revoked })))
}

async fn me(auth: Auth) -> ApiResult<Json<MeBody>> {
    Ok(Json(MeBody {
        id: auth.user.id,
        email: auth.user.email,
        display_name: auth.user.display_name,
        locale: auth.user.locale,
    }))
}
