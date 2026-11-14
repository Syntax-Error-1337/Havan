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

    // A client id must be unique per process. Two API replicas sharing "haven-api" evict each
    // other from the broker on every reconnect, which flips `/healthz` between ok and degraded
    // and drops the loser's connection mid-publish. Found by starting a second replica.
    let client_id = match std::env::var("HAVEN_INSTANCE") {
        Ok(name) if !name.trim().is_empty() => format!("haven-api-{}", name.trim()),
        _ => format!("haven-api-{}", std::process::id()),
    };
    let mqtt = infra::Mqtt::connect(&cfg.mqtt_url, &client_id).context("mqtt")?;
    let state = api::AppState::new(cfg.clone(), pg, cache, mqtt);
    let app = api::router(state);

    let listener = TcpListener::bind(cfg.bind).await.context("bind")?;
    info!(target: "haven::api", "listening on http://{}", cfg.bind);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
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
