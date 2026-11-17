use domain::theme::{AppTheme, ThemeMode};
use kernel::{AppError, TenantCtx};
use time::OffsetDateTime;

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
}

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
        Self { pg }
    }

    pub async fn load(&self) -> Result<AppTheme, AppError> {
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
        Ok(row.map(from_row).unwrap_or_default())
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
        Ok(from_row(row))
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
