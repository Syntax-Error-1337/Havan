use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use domain::api_key::{ApiKey, ApiKeyRepo, mint};
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;
use crate::auth::VendorAuth;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/vendor/api-keys", get(list).post(create))
        .route("/vendor/api-keys/{id}", delete(revoke))
}

#[derive(Deserialize)]
struct CreateBody {
    name: String,
}

/// Mint a key. The secret appears in this response and never again — losing it
/// means revoking and minting another, which is the point of storing a digest.
async fn create(
    State(state): State<AppState>,
    vendor: VendorAuth,
    peer: crate::limits::Peer,
    Json(body): Json<CreateBody>,
) -> ApiResult<(StatusCode, Json<ApiKey>)> {
    if body.name.trim().is_empty() {
        return Err(ApiError(kernel::AppError::validation(
            "give the key a name",
        )));
    }

    let id = kernel::new_id();
    let key = mint(id, &body.name);

    state
        .api_keys
        .create(vendor.vendor_id, &key)
        .await
        .map_err(ApiError)?;

    state
        .audit
        .record(
            domain::audit::AuditEntry::new(
                vendor.vendor_id,
                domain::audit::ActorKind::User,
                "api_key.created",
                "api_key",
            )
            .by(vendor.user_id)
            .on(id)
            .with_request(&vendor.request_id)
            .from_ip(peer.ip())
            // The secret itself is never in the trail; the name is enough to identify it.
            .detail(serde_json::json!({ "name": key.name })),
        )
        .await;

    Ok((
        StatusCode::CREATED,
        Json(ApiKey {
            id,
            name: key.name,
            prefix: key.prefix,
            secret: Some(key.secret),
            created_at: kernel::now(),
            last_used_at: None,
            revoked_at: None,
        }),
    ))
}

async fn list(State(state): State<AppState>, vendor: VendorAuth) -> ApiResult<Json<Vec<ApiKey>>> {
    Ok(Json(
        state
            .api_keys
            .list(vendor.vendor_id)
            .await
            .map_err(ApiError)?,
    ))
}

async fn revoke(
    State(state): State<AppState>,
    vendor: VendorAuth,
    peer: crate::limits::Peer,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    state
        .api_keys
        .revoke(vendor.vendor_id, id)
        .await
        .map_err(ApiError)?;

    // Revoking a key is the action a vendor is most likely to be asked about later.
    state
        .audit
        .record(
            domain::audit::AuditEntry::new(
                vendor.vendor_id,
                domain::audit::ActorKind::User,
                "api_key.revoked",
                "api_key",
            )
            .by(vendor.user_id)
            .on(id)
            .with_request(&vendor.request_id)
            .from_ip(peer.ip()),
        )
        .await;

    Ok(StatusCode::NO_CONTENT)
}
