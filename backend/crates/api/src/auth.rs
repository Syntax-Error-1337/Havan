//! Authentication extractors.
//!
//! A handler that takes `Auth` cannot run without a customer session, and one
//! that takes `VendorAuth` cannot run without a vendor session. The compiler
//! enforces it: forgetting the check is not a code-review question.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use domain::identity::{Claims, User};

use crate::app::AppState;
use crate::error::ApiError;

pub struct Auth {
    pub user: User,
    pub request_id: String,
}

pub struct VendorAuth {
    pub vendor_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub request_id: String,
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = claims(parts, state).await?;

        // A vendor token is not a customer token: it may not read a customer's
        // devices just because it is validly signed.
        if claims.vid.is_some() {
            return Err(ApiError(kernel::AppError::forbidden(
                "this endpoint is for customer sessions",
            )));
        }

        let user = state.identity.user(claims.sub).await.map_err(ApiError)?;
        Ok(Auth {
            user,
            request_id: request_id(parts),
        })
    }
}

impl FromRequestParts<AppState> for VendorAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let claims = claims(parts, state).await?;

        let vendor_id = claims.vid.ok_or_else(|| {
            ApiError(kernel::AppError::forbidden(
                "this endpoint requires a vendor session",
            ))
        })?;

        Ok(VendorAuth {
            vendor_id,
            user_id: claims.sub,
            request_id: request_id(parts),
        })
    }
}

async fn claims(parts: &Parts, state: &AppState) -> Result<Claims, ApiError> {
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

    state.identity.verify_access(token).map_err(ApiError)
}

fn request_id(parts: &Parts) -> String {
    parts
        .headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string()
}
