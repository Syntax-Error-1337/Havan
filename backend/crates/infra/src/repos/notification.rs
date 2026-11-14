use domain::notification::{
    CertificateWarning, Delivery, Notification, NotificationRepo, PushRepo, PushToken,
};
use kernel::{AppError, TenantCtx};
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

    async fn notify_certificate_expiry(
        &self,
        warning: &CertificateWarning,
    ) -> Result<u64, AppError> {
        let Some(home_id) = warning.home_id else {
            return Ok(0);
        };

        let mut tx = self.pg.scoped(&TenantCtx::system("cert-expiry")).await?;

        // One row per member, and none if that member already has this warning. Done in SQL
        // because the alternative is a read, a compare and an insert per person, which is
        // three ways to race.
        let inserted = sqlx::query(
            r#"INSERT INTO notifications (id, user_id, kind, title, body, data)
               SELECT gen_random_uuid(), m.user_id, 'security', $2, $3,
                      jsonb_build_object(
                          'device_id', $4::text,
                          'serial', $5::text,
                          'bucket_days', $6::int,
                          'days_left', $7::int)
                 FROM home_members m
                WHERE m.home_id = $1
                  AND NOT EXISTS (
                        SELECT 1 FROM notifications n
                         WHERE n.user_id = m.user_id
                           AND n.kind = 'security'
                           AND n.data->>'device_id' = $4::text
                           AND n.data->>'bucket_days' = $6::text)"#,
        )
        .bind(home_id)
        .bind(warning.title())
        .bind(warning.body())
        .bind(warning.device_id.to_string())
        .bind(&warning.serial)
        .bind(warning.bucket as i32)
        .bind(warning.days_left as i32)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();

        tx.commit().await.map_err(db)?;
        Ok(inserted)
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

impl PushRepo for PgNotifications {
    async fn undelivered(&self, limit: i64) -> Result<Vec<Delivery>, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("push")).await?;
        let rows: Vec<(Uuid, Uuid, String, String, String, serde_json::Value)> = sqlx::query_as(
            r#"SELECT id, user_id, kind, title, body, data
                 FROM notifications
                WHERE delivered_at IS NULL
                  AND delivery_attempts < $2
                ORDER BY created_at
                LIMIT $1"#,
        )
        .bind(limit)
        .bind(domain::notification::MAX_ATTEMPTS)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .map(|(id, user_id, kind, title, body, data)| Delivery {
                id,
                user_id,
                kind,
                title,
                body,
                data,
            })
            .collect())
    }

    async fn tokens_for(&self, user_id: Uuid) -> Result<Vec<PushToken>, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("push")).await?;
        let rows: Vec<(Uuid, Uuid, String, String, Option<String>)> = sqlx::query_as(
            r#"SELECT id, user_id, platform, token, app_version
                 FROM push_tokens WHERE user_id = $1"#,
        )
        .bind(user_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .map(|(id, user_id, platform, token, app_version)| PushToken {
                id,
                user_id,
                platform,
                token,
                app_version,
            })
            .collect())
    }

    async fn mark_delivered(&self, id: Uuid) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("push")).await?;
        sqlx::query("UPDATE notifications SET delivered_at = now() WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    async fn mark_attempt(&self, id: Uuid, error: &str) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("push")).await?;
        // Settled in the same statement: a row that has used its attempts is marked delivered
        // with the reason kept, so it leaves the queue but the history is still there.
        sqlx::query(
            r#"UPDATE notifications
                  SET delivery_attempts = delivery_attempts + 1,
                      delivery_error = $2,
                      delivered_at = CASE
                          WHEN delivery_attempts + 1 >= $3 THEN now()
                          ELSE delivered_at END
                WHERE id = $1"#,
        )
        .bind(id)
        .bind(error.chars().take(300).collect::<String>())
        .bind(domain::notification::MAX_ATTEMPTS)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    async fn prune_token(&self, id: Uuid) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("push")).await?;
        sqlx::query("DELETE FROM push_tokens WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    async fn register_token(
        &self,
        user_id: Uuid,
        platform: &str,
        token: &str,
        app_version: Option<&str>,
    ) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("push")).await?;
        sqlx::query(
            r#"INSERT INTO push_tokens (id, user_id, platform, token, app_version)
               VALUES ($1, $2, $3, $4, $5)
               ON CONFLICT (user_id, token)
               DO UPDATE SET platform = EXCLUDED.platform, app_version = EXCLUDED.app_version"#,
        )
        .bind(kernel::new_id())
        .bind(user_id)
        .bind(platform)
        .bind(token)
        .bind(app_version)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    async fn remove_token(&self, user_id: Uuid, token: &str) -> Result<bool, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("push")).await?;
        let removed = sqlx::query("DELETE FROM push_tokens WHERE user_id = $1 AND token = $2")
            .bind(user_id)
            .bind(token)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        tx.commit().await.map_err(db)?;
        Ok(removed > 0)
    }
}
