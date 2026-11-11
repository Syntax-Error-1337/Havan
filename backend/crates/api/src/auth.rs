//! Authentication extractor.
//!
//! A handler that takes `Auth` cannot run without a valid access token, and the
//! compiler enforces it: forgetting the check is not a code-review question.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use domain::identity::User;

use crate::app::AppState;
use crate::error::ApiError;

pub struct Auth {
    pub user: User,
    pub request_id: String,
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                ApiError(kernel::AppError::code(
                    kernel::ErrorCode::Unauthorized,
                    "missing bearer token",
                ))
            })?;

        let token = header.strip_prefix("Bearer ").ok_or_else(|| {
            ApiError(kernel::AppError::code(
                kernel::ErrorCode::Unauthorized,
                "authorization must be a bearer token",
            ))
        })?;

        let claims = state.identity.verify_access(token).map_err(ApiError)?;
        let user = state.identity.user(claims.sub).await.map_err(ApiError)?;

        let request_id = parts
            .headers
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        Ok(Auth { user, request_id })
    }
}
