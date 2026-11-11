//! Authentication extractors.
//!
//! A handler that takes `Auth` cannot run without a customer session, and one
//! that takes `VendorAuth` cannot run without a vendor session. The compiler
//! enforces it: forgetting the check is not a code-review question.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use domain::api_key::ApiKeyRepo;
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
        let token = bearer(parts)?;

        // A key and a session are both "a vendor acting". A key has no user and
        // no expiry of its own — it is revoked instead, which is why it is
        // checked against the database rather than verified as a JWT.
        if let Some((prefix, secret)) = domain::api_key::parse(token) {
            let vendor_id = state
                .api_keys
                .authenticate(prefix, secret)
                .await
                .map_err(ApiError)?
                .ok_or_else(|| {
                    ApiError(kernel::AppError::code(
                        kernel::ErrorCode::Unauthorized,
                        "that api key is not valid",
                    ))
                })?;

            return Ok(VendorAuth {
                vendor_id,
                // A key is not a person; the nil uuid says so rather than being
                // attributed to whoever created it.
                user_id: uuid::Uuid::nil(),
                request_id: request_id(parts),
            });
        }

        let claims = state.identity.verify_access(token).map_err(ApiError)?;

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

fn bearer(parts: &Parts) -> Result<&str, ApiError> {
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

    header.strip_prefix("Bearer ").ok_or_else(|| {
        ApiError(kernel::AppError::code(
            kernel::ErrorCode::Unauthorized,
            "authorization must be a bearer token",
        ))
    })
}

async fn claims(parts: &Parts, state: &AppState) -> Result<Claims, ApiError> {
    state
        .identity
        .verify_access(bearer(parts)?)
        .map_err(ApiError)
}

fn request_id(parts: &Parts) -> String {
    parts
        .headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string()
}
