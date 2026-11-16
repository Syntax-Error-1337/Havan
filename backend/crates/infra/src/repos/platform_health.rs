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
    /// The devices that have been silent longest — the answer to "which ones", not "how many".
    pub quietest: Vec<QuietDevice>,
    /// Command requests the platform could not deliver, oldest first.
    pub stuck_commands: Vec<StuckCommand>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuietDevice {
    pub serial: Option<String>,
    pub name: String,
    pub vendor: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_seen_at: Option<time::OffsetDateTime>,
    pub silent_minutes: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StuckCommand {
    pub device: Option<String>,
    pub vendor: String,
    pub op: String,
    pub attempts: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub requested_at: time::OffsetDateTime,
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

        // Five each, not a page: a health screen is for the first look, and a list long enough to
        // scroll is a list somebody abandons.
        type QuietRow = (
            Option<String>,
            String,
            String,
            Option<time::OffsetDateTime>,
            i64,
        );
        type StuckRow = (Option<String>, String, String, i32, time::OffsetDateTime);

        let quietest: Vec<QuietRow> = sqlx::query_as(
            r#"SELECT d.serial, d.name, v.name, d.last_seen_at,
                          (EXTRACT(EPOCH FROM (now() - d.last_seen_at)) / 60)::bigint
                     FROM devices d JOIN vendors v ON v.id = d.vendor_id
                    WHERE d.last_seen_at IS NOT NULL
                    ORDER BY d.last_seen_at ASC
                    LIMIT 5"#,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;

        let stuck: Vec<StuckRow> = sqlx::query_as(
            r#"SELECT d.serial, v.name, c.op, c.attempts, c.requested_at
                 FROM commands c
                 JOIN vendors v ON v.id = c.vendor_id
                 LEFT JOIN devices d ON d.id = c.device_id
                WHERE c.status = 'unreachable'
                ORDER BY c.requested_at ASC
                LIMIT 5"#,
        )
        .fetch_all(&mut *tx)
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
            quietest: quietest
                .into_iter()
                .map(
                    |(serial, name, vendor, last_seen_at, silent_minutes)| QuietDevice {
                        serial,
                        name,
                        vendor,
                        last_seen_at,
                        silent_minutes,
                    },
                )
                .collect(),
            stuck_commands: stuck
                .into_iter()
                .map(
                    |(device, vendor, op, attempts, requested_at)| StuckCommand {
                        device,
                        vendor,
                        op,
                        attempts,
                        requested_at,
                    },
                )
                .collect(),
        })
    }
}
