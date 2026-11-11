use domain::notification::{Notification, NotificationRepo};
use kernel::AppError;
use uuid::Uuid;

use crate::pg::Pg;

#[derive(Clone)]
pub struct PgNotifications {
    pg: Pg,
}

impl PgNotifications {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

type Row = (
    Uuid,
    String,
    String,
    String,
    serde_json::Value,
    time::OffsetDateTime,
    Option<time::OffsetDateTime>,
);

fn map(row: Row) -> Notification {
    let (id, kind, title, body, data, created_at, read_at) = row;
    Notification {
        id,
        kind,
        title,
        body,
        data,
        created_at,
        read_at,
    }
}

const COLUMNS: &str = "id, kind, title, body, data, created_at, read_at";

impl NotificationRepo for PgNotifications {
    async fn list(
        &self,
        user_id: Uuid,
        unread_only: bool,
        limit: i64,
    ) -> Result<Vec<Notification>, AppError> {
        // Keyed by user, always. `notifications` has no vendor column and no RLS:
        // the only thing that keeps one person's alerts apart is this predicate,
        // so it is not optional.
        let rows = sqlx::query_as::<_, Row>(&format!(
            r#"SELECT {COLUMNS} FROM notifications
                WHERE user_id = $1 AND ($2 = false OR read_at IS NULL)
                ORDER BY created_at DESC
                LIMIT $3"#
        ))
        .bind(user_id)
        .bind(unread_only)
        .bind(limit.clamp(1, 200))
        .fetch_all(self.pg.pool())
        .await
        .map_err(db)?;

        Ok(rows.into_iter().map(map).collect())
    }

    async fn unread_count(&self, user_id: Uuid) -> Result<i64, AppError> {
        let count = sqlx::query_scalar(
            "SELECT count(*) FROM notifications WHERE user_id = $1 AND read_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(self.pg.pool())
        .await
        .map_err(db)?;

        Ok(count)
    }

    async fn mark_read(&self, user_id: Uuid, id: Uuid) -> Result<bool, AppError> {
        let affected = sqlx::query(
            "UPDATE notifications SET read_at = now() WHERE id = $1 AND user_id = $2 AND read_at IS NULL",
        )
        .bind(id)
        .bind(user_id)
        .execute(self.pg.pool())
        .await
        .map_err(db)?
        .rows_affected();

        if affected > 0 {
            return Ok(true);
        }

        // Nothing changed: either it was already read (fine) or it is not theirs
        // (not fine). Distinguish rather than returning success for both.
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM notifications WHERE id = $1 AND user_id = $2)",
        )
        .bind(id)
        .bind(user_id)
        .fetch_one(self.pg.pool())
        .await
        .map_err(db)?;

        if exists {
            Ok(false)
        } else {
            Err(AppError::not_found("notification"))
        }
    }

    async fn mark_all_read(&self, user_id: Uuid) -> Result<u64, AppError> {
        let affected = sqlx::query(
            "UPDATE notifications SET read_at = now() WHERE user_id = $1 AND read_at IS NULL",
        )
        .bind(user_id)
        .execute(self.pg.pool())
        .await
        .map_err(db)?
        .rows_affected();

        Ok(affected)
    }
}
