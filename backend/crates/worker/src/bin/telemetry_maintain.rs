//! `haven-telemetry-maintain` — roll the telemetry window now, and print what moved.
//!
//! The worker does this every six hours. This binary exists for the two cases where waiting is not an
//! option: an operator watching a disk fill, and a person checking that the promise in the docs ("old
//! months are dropped, not deleted") is actually true of the running database.

use anyhow::Context;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = kernel::Config::from_env().context("configuration")?;
    kernel::telemetry::init(&cfg.env);

    // DDL, so it runs with the schema owner's credentials — the same rule the migrator follows.
    let url = std::env::var("MIGRATE_DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| cfg.database_url.clone());

    let retention_days = std::env::var("TELEMETRY_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(infra::repos::telemetry_partitions::DEFAULT_RETENTION_DAYS);

    let pg = infra::Pg::connect(&url, 1).await.context("connect")?;
    let report = infra::repos::telemetry_partitions::maintain(&pg, retention_days)
        .await
        .context("maintain")?;

    tracing::info!(
        created = ?report.created,
        dropped = ?report.dropped,
        overflow_rows = report.overflow_rows,
        retention_days,
        "telemetry window rolled"
    );
    Ok(())
}
