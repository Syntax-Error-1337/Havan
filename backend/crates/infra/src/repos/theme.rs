use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use domain::theme::{AppTheme, ThemeMode};
use kernel::{AppError, TenantCtx};
use time::OffsetDateTime;
use tokio::sync::Mutex;

use crate::pg::Pg;

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

/// One saved theme, as PostgreSQL returns it.
type Row = (
    String,
    String,
    String,
    String,
    String,
    OffsetDateTime,
    Option<String>,
);

/// Platform appearance, stored.
///
/// One row, read by every client, written only by the operator console. Reads run as `haven_system`
/// because the row belongs to no tenant: it is the same answer for everybody, including the sign-in
/// screen, which is fetched before there is a session to scope by.
#[derive(Clone)]
pub struct PgTheme {
    pg: Pg,
    /// The last answer, held in this process for a moment.
    ///
    /// The theme is read on every cold start of every install and changes when one person presses
    /// Save. A million launches is a million single-row reads for an answer that has not moved, and
    /// the HTTP cache already says so to the client — this says it to the database. Thirty seconds is
    /// short enough that an operator's change is live before they can reload the app twice.
    cache: Arc<Mutex<Option<(AppTheme, Instant)>>>,
    hits: Arc<AtomicU64>,
}

/// How long a loaded theme is trusted without asking the database again.
const CACHE_TTL: Duration = Duration::from_secs(30);

/// One entry of the change log, as the console shows it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ThemeChange {
    #[serde(with = "time::serde::rfc3339")]
    pub changed_at: OffsetDateTime,
    pub changed_by: String,
    pub fields: Vec<String>,
}

impl PgTheme {
    pub fn new(pg: Pg) -> Self {
        Self {
            pg,
            cache: Arc::new(Mutex::new(None)),
            hits: Arc::new(AtomicU64::new(0)),
        }
    }

    /// How many reads were answered without touching the database. Exposed so `/metrics` can show the
    /// cache doing its job — a cache nobody can see is a cache nobody knows is working.
    pub fn cache_hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    pub async fn load(&self) -> Result<AppTheme, AppError> {
        if let Some(theme) = self.fresh_from_cache().await {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return Ok(theme);
        }

        let mut tx = self.pg.scoped(&TenantCtx::system("theme-read")).await?;

        let row: Option<Row> = sqlx::query_as(
            r#"SELECT product_name, logo_text, accent, accent_secondary, mode, updated_at, updated_by
                 FROM app_theme WHERE id = 1"#,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        // A missing row is the built-in theme, not an error: a fresh database serves the product's
        // own colours until an operator chooses otherwise.
        let theme = row.map(from_row).unwrap_or_default();
        self.remember(&theme).await;
        Ok(theme)
    }

    /// The cached theme, if it is still young enough to trust.
    async fn fresh_from_cache(&self) -> Option<AppTheme> {
        let guard = self.cache.lock().await;
        match guard.as_ref() {
            Some((theme, at)) if at.elapsed() < CACHE_TTL => Some(theme.clone()),
            _ => None,
        }
    }

    async fn remember(&self, theme: &AppTheme) {
        *self.cache.lock().await = Some((theme.clone(), Instant::now()));
    }

    /// Save, and keep what it was.
    ///
    /// The history row is written in the same transaction as the update, so a change and its receipt
    /// cannot disagree; an unchanged save writes no history, because a log full of "nobody changed
    /// anything" is a log nobody reads.
    pub async fn save(&self, theme: &AppTheme, actor: &str) -> Result<AppTheme, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("theme-write")).await?;

        let previous: Option<Row> = sqlx::query_as(
            r#"SELECT product_name, logo_text, accent, accent_secondary, mode, updated_at, updated_by
                 FROM app_theme WHERE id = 1 FOR UPDATE"#,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        let before = previous.map(from_row).unwrap_or_default();
        let changed = theme.changes(&before);

        let row: Row = sqlx::query_as(
            r#"INSERT INTO app_theme
                 (id, product_name, logo_text, accent, accent_secondary, mode, updated_at, updated_by)
               VALUES (1, $1, $2, $3, $4, $5, now(), $6)
               ON CONFLICT (id) DO UPDATE
                 SET product_name = EXCLUDED.product_name,
                     logo_text = EXCLUDED.logo_text,
                     accent = EXCLUDED.accent,
                     accent_secondary = EXCLUDED.accent_secondary,
                     mode = EXCLUDED.mode,
                     updated_at = now(),
                     updated_by = EXCLUDED.updated_by
               RETURNING product_name, logo_text, accent, accent_secondary, mode, updated_at,
                         updated_by"#,
        )
        .bind(&theme.product_name)
        .bind(&theme.logo_text)
        .bind(&theme.accent)
        .bind(&theme.accent_secondary)
        .bind(theme.mode.as_str())
        .bind(actor)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;

        if !changed.is_empty() {
            sqlx::query(
                r#"INSERT INTO app_theme_history (id, changed_by, previous, next)
                   VALUES ($1, $2, $3, $4)"#,
            )
            .bind(kernel::new_id())
            .bind(actor)
            .bind(serde_json::to_value(&before).unwrap_or_default())
            .bind(serde_json::to_value(theme_for_history(theme)).unwrap_or_default())
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }

        tx.commit().await.map_err(db)?;

        // The writer replaces the cache rather than dropping it: the next read — the operator's own
        // rerender, most likely — gets what they just saved, not what a TTL remembered.
        let saved = from_row(row);
        self.remember(&saved).await;
        Ok(saved)
    }

    pub async fn history(&self, limit: i64) -> Result<Vec<ThemeChange>, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("theme-history")).await?;

        let rows: Vec<(OffsetDateTime, String, serde_json::Value, serde_json::Value)> =
            sqlx::query_as(
                r#"SELECT changed_at, changed_by, previous, next
                     FROM app_theme_history ORDER BY changed_at DESC LIMIT $1"#,
            )
            .bind(limit.clamp(1, 100))
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;

        Ok(rows
            .into_iter()
            .map(|(changed_at, changed_by, previous, next)| ThemeChange {
                changed_at,
                changed_by,
                fields: differing_fields(&previous, &next),
            })
            .collect())
    }
}

/// The stored columns, without the derived text colour — it is recomputed on read, so a change to the
/// contrast rule applies to an old theme as well.
fn from_row(row: Row) -> AppTheme {
    let (product_name, logo_text, accent, accent_secondary, mode, updated_at, updated_by) = row;
    AppTheme {
        accent_on: domain::theme::on_colour(&accent).to_string(),
        product_name,
        logo_text,
        accent,
        accent_secondary,
        mode: ThemeMode::parse(&mode).unwrap_or(ThemeMode::Dark),
        updated_at: Some(updated_at),
        updated_by,
    }
}

/// The history row stores what was *asked for*, so the diff names the same fields the operator saw.
fn theme_for_history(theme: &AppTheme) -> serde_json::Value {
    serde_json::json!({
        "product_name": theme.product_name,
        "logo_text": theme.logo_text,
        "accent": theme.accent,
        "accent_secondary": theme.accent_secondary,
        "mode": theme.mode.as_str(),
    })
}

fn differing_fields(previous: &serde_json::Value, next: &serde_json::Value) -> Vec<String> {
    [
        "product_name",
        "logo_text",
        "accent",
        "accent_secondary",
        "mode",
    ]
    .into_iter()
    .filter(|key| previous.get(*key) != next.get(*key))
    .map(str::to_string)
    .collect()
}
