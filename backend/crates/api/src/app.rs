//! Router composition and shared state.
//!
//! Feature routers are mounted here as they land, each behind its own
//! `routes::<slice>::router()`, so this file stays a table of contents.

use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use domain::devices::DeviceService;
use domain::homes::HomeService;
use domain::identity::IdentityService;
use domain::pairing::PairingService;
use infra::{Cache, Pg, PgDevices, PgHomes, PgIdentity, PgPairing};
use kernel::Config;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

use crate::health;
use crate::routes;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub pg: Pg,
    pub cache: Cache,
    pub identity: Arc<IdentityService<PgIdentity>>,
    pub homes: Arc<HomeService<PgHomes>>,
    pub devices: Arc<DeviceService<PgDevices, PgHomes>>,
    pub pairing: Arc<PairingService<PgPairing>>,
}

impl AppState {
    pub fn new(cfg: Arc<Config>, pg: Pg, cache: Cache) -> Self {
        let pool = pg.pool().clone();
        let identity = Arc::new(IdentityService::new(
            Arc::new(PgIdentity::new(pool.clone())),
            cfg.jwt_secret.expose(),
        ));
        let homes = Arc::new(HomeService::new(Arc::new(PgHomes::new(pool.clone()))));
        let devices = Arc::new(DeviceService::new(
            Arc::new(PgDevices::new(pg.clone())),
            homes.clone(),
        ));
        let pairing = Arc::new(PairingService::new(
            Arc::new(PgPairing::new(pg.clone())),
            cfg.ticket_secret.expose(),
        ));
        Self {
            cfg,
            pg,
            cache,
            identity,
            homes,
            devices,
            pairing,
        }
    }
}

pub fn router(state: AppState) -> Router {
    let v1 = Router::new()
        .merge(routes::auth::router())
        .merge(routes::homes::router())
        .merge(routes::devices::router())
        .merge(routes::pairing::router())
        .route("/stream", get(routes::stream::ws));

    Router::new()
        .route("/healthz", get(health::health))
        .route("/livez", get(health::live))
        .nest("/v1", v1)
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(tower_http::trace::DefaultMakeSpan::new().include_headers(false)),
        )
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}
