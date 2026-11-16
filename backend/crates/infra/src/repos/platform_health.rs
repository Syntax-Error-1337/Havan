use kernel::{AppError, TenantCtx};
use serde::Serialize;

use crate::pg::Pg;

/// The numbers an on-call rotation opens first.
///
/// Deliberately a handful of counts and nothing else: every extra figure here is one somebody has to
/// interpret at 3am, and a health page that needs interpretation is a health page nobody reads.
#[derive(Debug, Clone, Serialize)]
pub struct PlatformHealth {
    pub devices: i64,
    /// Flagged online by the platform's own view (a shadow with `online: true`).
    pub online: i64,
    /// Reported in the last fifteen minutes — the figure that disagrees with `online` when a will
    /// message never arrived, which is the failure worth seeing.
    pub seen_recently: i64,
    /// Telemetry points in the last hour.
    pub points_last_hour: i64,
    /// Commands requested in the last hour.
    pub commands_last_hour: i64,
    /// Commands the platform refused to send, waiting to be redelivered.
    pub undelivered: i64,
    /// Notifications written but not yet delivered to a phone.
    pub undelivered_notifications: i64,
    /// Webhook deliveries waiting on a vendor's endpoint.
    pub webhook_backlog: i64,
}

#[derive(Clone)]
pub struct PgPlatformHealth {
    pg: Pg,
}

impl PgPlatformHealth {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    pub async fn read(&self) -> Result<PlatformHealth, AppError> {
        let mut tx = self
            .pg
            .scoped(&TenantCtx::system("platform-health"))
            .await?;
        let row: (i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
            r#"SELECT
                 (SELECT count(*) FROM devices),
                 (SELECT count(*) FROM devices WHERE online),
                 (SELECT count(*) FROM devices WHERE last_seen_at > now() - interval '15 minutes'),
                 (SELECT count(*) FROM telemetry WHERE ts > now() - interval '1 hour'),
                 (SELECT count(*) FROM commands WHERE requested_at > now() - interval '1 hour'),
                 (SELECT count(*) FROM commands WHERE status = 'unreachable'),
                 (SELECT count(*) FROM notifications WHERE delivered_at IS NULL),
                 (SELECT count(*) FROM webhook_outbox WHERE delivered_at IS NULL)"#,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(Box::new(e)))?;

        Ok(PlatformHealth {
            devices: row.0,
            online: row.1,
            seen_recently: row.2,
            points_last_hour: row.3,
            commands_last_hour: row.4,
            undelivered: row.5,
            undelivered_notifications: row.6,
            webhook_backlog: row.7,
        })
    }
}
