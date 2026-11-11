//! `haven-migrate` — applies the schema and exits. Owning migrations in one
//! binary (rather than in `haven-api` at boot) means a rolling deploy never has
//! two pods racing the same DDL.

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use std::path::Path;

/// Resolved at compile time so the binary works from any working directory.
const MIGRATIONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../migrations");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = kernel::Config::from_env().context("configuration")?;
    kernel::telemetry::init(&cfg.env);

    // DDL runs as the schema owner. The application role is deliberately not a
    // superuser, so that row-level security applies to it.
    let url = std::env::var("MIGRATE_DATABASE_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| cfg.database_url.clone());

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .context("connect")?;

    // Loaded at runtime, not with `sqlx::migrate!`: the macro embeds the file
    // list at compile time, so a *new* migration file is silently skipped until
    // the crate happens to rebuild. Reading the directory cannot go stale.
    let migrator = sqlx::migrate::Migrator::new(Path::new(MIGRATIONS_DIR))
        .await
        .context("read migrations")?;
    migrator.run(&pool).await.context("migrate")?;

    tracing::info!(known = migrator.iter().count(), "migrations applied");
    Ok(())
}
