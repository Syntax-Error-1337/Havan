//! Router composition and shared state.
//!
//! Feature routers are mounted here as they land, each behind its own
//! `routes::<slice>::router()`, so this file stays a table of contents.

use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use domain::automation::AutomationService;
use domain::catalog::CatalogService;
use domain::devices::DeviceService;
use domain::homes::HomeService;
use domain::identity::IdentityService;
use domain::pairing::PairingService;
use infra::{
    Cache, Mqtt, Pg, PgApiKeys, PgAutomation, PgCatalog, PgDevices, PgHistory, PgHomes, PgIdentity,
    PgNotifications, PgOta, PgPairing, PgScenes,
};
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
    pub mqtt: Mqtt,
    pub catalog: Arc<CatalogService<PgCatalog>>,
    pub automations: Arc<AutomationService<PgAutomation>>,
    pub ota: Arc<PgOta>,
    pub api_keys: Arc<PgApiKeys>,
    pub history: Arc<PgHistory>,
    pub notifications: Arc<PgNotifications>,
    pub scenes: Arc<PgScenes>,
}

impl AppState {
    pub fn new(cfg: Arc<Config>, pg: Pg, cache: Cache, mqtt: Mqtt) -> Self {
        let pool = pg.pool().clone();
        let identity = Arc::new(IdentityService::new(
            Arc::new(PgIdentity::new(pg.clone())),
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
        let catalog = Arc::new(CatalogService::new(Arc::new(PgCatalog::new(pg.clone()))));
        let ota = Arc::new(PgOta::new(pg.clone()));
        let api_keys = Arc::new(PgApiKeys::new(pg.clone()));
        let history = Arc::new(PgHistory::new(pg.clone()));
        let notifications = Arc::new(PgNotifications::new(pg.clone()));
        let scenes = Arc::new(PgScenes::new(pg.clone()));
        let automations = Arc::new(AutomationService::new(Arc::new(PgAutomation::new(
            pg.clone(),
        ))));
        Self {
            cfg,
            pg,
            cache,
            identity,
            homes,
            devices,
            pairing,
            mqtt,
            catalog,
            automations,
            ota,
            api_keys,
            history,
            notifications,
            scenes,
        }
    }
}

pub fn router(state: AppState) -> Router {
    let v1 = Router::new()
        .merge(routes::auth::router())
        .merge(routes::homes::router())
        .merge(routes::devices::router())
        .merge(routes::vendor::router())
        .merge(routes::pairing::router())
        .merge(routes::automations::router())
        .merge(routes::catalog::router())
        .merge(routes::ota::router())
        .merge(routes::api_keys::router())
        .merge(routes::notifications::router())
        .merge(routes::scenes::router())
        .route(
            "/devices/{id}/history",
            axum::routing::get(routes::history::history),
        )
        .route("/stream", get(routes::stream::ws));

    let app = Router::new()
        .route("/healthz", get(health::health))
        .route("/livez", get(health::live))
        .nest("/v1", v1);

    // CORS exists for one reason: the customer app has a web build used to check
    // layout and flows without a simulator. A native app sends no Origin, so this
    // changes nothing for production traffic — but it is dev-only on purpose, and
    // a deployment that needs a browser client should put its own origin here.
    if cfg!(debug_assertions) {
        app.layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        )
    } else {
        app
    }
    .with_state(state)
    .layer(
        TraceLayer::new_for_http()
            .make_span_with(tower_http::trace::DefaultMakeSpan::new().include_headers(false)),
    )
    .layer(PropagateRequestIdLayer::x_request_id())
    .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
}
