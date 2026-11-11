//! Router composition and shared state.
//!
//! Feature routers are mounted here as they land, each behind its own
//! `routes::<slice>::router()`, so this file stays a table of contents.

use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use infra::{Cache, Pg};
use kernel::Config;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

use crate::health;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub pg: Pg,
    pub cache: Cache,
}

impl AppState {
    pub fn new(cfg: Arc<Config>, pg: Pg, cache: Cache) -> Self {
        Self { cfg, pg, cache }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health::health))
        .route("/livez", get(health::live))
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(tower_http::trace::DefaultMakeSpan::new().include_headers(false)),
        )
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}
