//! `haven-api` — the process that serves humans.

use std::sync::Arc;

use anyhow::Context;
use kernel::Config;
use kernel::config::Env;
use tokio::net::TcpListener;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = Arc::new(Config::from_env().context("configuration")?);
    kernel::telemetry::init(&cfg.env);

    info!(
        target: "haven::api",
        addr = %cfg.bind,
        env = ?cfg.env,
        "starting"
    );

    let pg = infra::Pg::connect(&cfg.database_url, cfg.db_max_connections)
        .await
        .context("postgres")?;
    let cache = infra::Cache::connect(&cfg.redis_url)
        .await
        .context("redis")?;

    let mqtt = infra::Mqtt::connect(&cfg.mqtt_url, "haven-api").context("mqtt")?;
    let state = api::AppState::new(cfg.clone(), pg, cache, mqtt);
    let app = api::router(state);

    let listener = TcpListener::bind(cfg.bind).await.context("bind")?;
    info!(target: "haven::api", "listening on http://{}", cfg.bind);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await
        .context("serve")?;

    if cfg.env == Env::Prod {
        info!("shutdown complete");
    }
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let term = async {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut s) = signal(SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
    info!("shutting down");
}
