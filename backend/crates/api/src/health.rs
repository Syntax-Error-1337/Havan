//! `/healthz` and `/readyz`.
//!
//! Two endpoints on purpose: a liveness probe must not fail because Postgres is
//! slow (that restarts a healthy pod), while a readiness probe must fail (that
//! removes it from the load balancer).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::AppState;

#[derive(Debug, Serialize)]
pub struct ComponentHealth {
    pub db: bool,
    pub redis: bool,
    /// Without the broker the API can still serve reads, but every command it
    /// accepts has nowhere to go — so it is part of readiness, not an aside.
    pub mqtt: bool,
}

#[derive(Debug, Serialize)]
pub struct Health {
    pub status: &'static str,
    pub components: ComponentHealth,
    pub version: &'static str,
}

impl Health {
    pub fn is_ok(&self) -> bool {
        self.components.db && self.components.redis && self.components.mqtt
    }
}

pub async fn health(State(state): State<AppState>) -> Response {
    let (db, redis) = tokio::join!(state.pg.ping(), state.cache.ping());
    let mqtt = state.mqtt.is_connected();
    let health = Health {
        status: if db && redis && mqtt {
            "ok"
        } else {
            "degraded"
        },
        components: ComponentHealth { db, redis, mqtt },
        version: env!("CARGO_PKG_VERSION"),
    };
    let code = if health.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(health)).into_response()
}

/// Liveness: the process is up and the event loop is turning. Never touches a
/// dependency.
pub async fn live() -> StatusCode {
    StatusCode::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_degraded_dependency_is_a_degraded_service() {
        let h = Health {
            status: "ok",
            components: ComponentHealth {
                db: true,
                redis: false,
                mqtt: true,
            },
            version: "0.1.0",
        };
        assert!(!h.is_ok());
    }
}
