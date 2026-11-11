use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::routing::get;
use domain::catalog::PublicProduct;
use serde::Deserialize;

use crate::app::AppState;
use crate::auth::Auth;
use crate::error::{ApiError, ApiResult};

pub fn router() -> Router<AppState> {
    Router::new().route("/catalog/products", get(lookup))
}

#[derive(Deserialize)]
struct LookupQuery {
    /// Comma-separated advertisement hashes (8 hex chars) or product keys.
    hashes: Option<String>,
    keys: Option<String>,
}

/// What the radar asks after it hears an advertisement: "who is `a1b2c3d4`?"
///
/// Bounded and exact — a radar knows a handful of hashes, not a catalogue, and
/// this must not become a way to enumerate every product on the platform.
async fn lookup(
    State(state): State<AppState>,
    _auth: Auth,
    Query(q): Query<LookupQuery>,
) -> ApiResult<Json<Vec<PublicProduct>>> {
    let mut terms: Vec<String> = Vec::new();
    for source in [q.hashes, q.keys].into_iter().flatten() {
        terms.extend(
            source
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string),
        );
    }

    Ok(Json(
        state
            .catalog
            .public_lookup(&terms)
            .await
            .map_err(ApiError)?,
    ))
}
