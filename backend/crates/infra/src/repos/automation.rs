use std::collections::HashMap;

use crate::pg::Pg;
use domain::automation::{Automation, AutomationDef, AutomationRepo, RunLog, RunRecord, Trigger};
use kernel::{AppError, TenantCtx};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone)]
pub struct PgAutomation {
    pg: Pg,
}

impl PgAutomation {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    async fn tx(&self) -> Result<Transaction<'static, Postgres>, AppError> {
        self.pg.scoped(&TenantCtx::system("automation")).await
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

type Row = (
    Uuid,
    Uuid,
    String,
    bool,
    serde_json::Value,
    Option<time::OffsetDateTime>,
);

fn map(row: Row) -> Result<Automation, AppError> {
    let (id, home_id, name, enabled, definition, next_run_at) = row;
    let definition: AutomationDef = serde_json::from_value(definition)
        .map_err(|e| AppError::Internal(format!("stored automation is invalid: {e}")))?;
    Ok(Automation {
        id,
        home_id,
        name,
        enabled,
        definition,
        next_run_at,
    })
}

const COLUMNS: &str = "id, home_id, name, enabled, definition, next_run_at";

impl AutomationRepo for PgAutomation {
    async fn create(
        &self,
        home_id: Uuid,
        name: &str,
        def: &AutomationDef,
        next_run_at: Option<time::OffsetDateTime>,
    ) -> Result<Automation, AppError> {
        let mut tx = self.tx().await?;
        let id = kernel::new_id();
        let definition = serde_json::to_value(def)
            .map_err(|e| AppError::Internal(format!("automation: {e}")))?;

        sqlx::query(
            r#"INSERT INTO automations (id, home_id, name, enabled, definition, next_run_at)
               VALUES ($1, $2, $3, true, $4, $5)"#,
        )
        .bind(id)
        .bind(home_id)
        .bind(name)
        .bind(&definition)
        .bind(next_run_at)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(Automation {
            id,
            home_id,
            name: name.to_string(),
            enabled: true,
            definition: def.clone(),
            next_run_at,
        })
    }

    async fn list(&self, home_id: Uuid) -> Result<Vec<Automation>, AppError> {
        let mut tx = self.tx().await?;
        let rows = sqlx::query_as::<_, Row>(&format!(
            "SELECT {COLUMNS} FROM automations WHERE home_id = $1 ORDER BY created_at DESC"
        ))
        .bind(home_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        rows.into_iter().map(map).collect()
    }

    async fn home_of(&self, id: Uuid) -> Result<Option<Uuid>, AppError> {
        let mut tx = self.tx().await?;
        let home = sqlx::query_scalar("SELECT home_id FROM automations WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(home)
    }

    async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        let mut tx = self.tx().await?;
        let n = sqlx::query("DELETE FROM automations WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        tx.commit().await.map_err(db)?;

        if n == 0 {
            return Err(AppError::not_found("automation"));
        }
        Ok(())
    }

    async fn set_enabled(&self, id: Uuid, enabled: bool) -> Result<Automation, AppError> {
        let mut tx = self.tx().await?;

        // Arming a schedule means `next_run_at = now()`: the scheduler picks it
        // up on its next tick and computes the real next run itself.
        let row = sqlx::query_as::<_, Row>(&format!(
            r#"UPDATE automations
                  SET enabled = $2,
                      next_run_at = CASE
                        WHEN $2 AND definition #>> '{{trigger,type}}' = 'schedule' THEN now()
                        ELSE NULL
                      END
                WHERE id = $1
            RETURNING {COLUMNS}"#
        ))
        .bind(id)
        .bind(enabled)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        row.map(map)
            .transpose()?
            .ok_or_else(|| AppError::not_found("automation"))
    }

    async fn runs(&self, automation_id: Uuid, limit: i64) -> Result<Vec<RunRecord>, AppError> {
        let mut tx = self.tx().await?;
        let rows = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                time::OffsetDateTime,
                serde_json::Value,
            ),
        >(
            r#"SELECT id, trigger_kind, status, started_at, log
                 FROM automation_runs
                WHERE automation_id = $1
                ORDER BY started_at DESC
                LIMIT $2"#,
        )
        .bind(automation_id)
        .bind(limit.clamp(1, 100))
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        rows.into_iter()
            .map(|(id, trigger_kind, status, started_at, log)| {
                // The column holds the steps array, not the RunLog wrapper.
                let steps: Vec<domain::automation::RunStep> = serde_json::from_value(log)
                    .map_err(|e| AppError::Internal(format!("stored run log: {e}")))?;
                let log = RunLog { steps };
                Ok(RunRecord {
                    id,
                    trigger_kind,
                    status,
                    started_at,
                    log,
                })
            })
            .collect()
    }

    async fn triggered_by(&self, device_id: Uuid, code: &str) -> Result<Vec<Automation>, AppError> {
        let mut tx = self.tx().await?;
        let rows = sqlx::query_as::<_, Row>(&format!(
            r#"SELECT {COLUMNS} FROM automations
                WHERE enabled
                  AND definition #>> '{{trigger,type}}' = 'device_state'
                  AND definition #>> '{{trigger,device_id}}' = $1
                  AND definition #>> '{{trigger,code}}' = $2"#
        ))
        .bind(device_id.to_string())
        .bind(code)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        rows.into_iter().map(map).collect()
    }

    /// The lease is a single UPDATE: whichever worker's statement wins the row
    /// owns it until `lease_until`. Two workers polling at the same instant
    /// cannot both fire the same automation.
    async fn lease_due(&self, owner: &str, ttl_secs: i64) -> Result<Vec<Automation>, AppError> {
        let mut tx = self.tx().await?;

        let rows = sqlx::query_as::<_, Row>(&format!(
            r#"UPDATE automations
                  SET lease_until = now() + make_interval(secs => $2),
                      lease_owner = $1
                WHERE id IN (
                    SELECT id FROM automations
                     WHERE enabled
                       AND next_run_at IS NOT NULL
                       AND next_run_at <= now()
                       AND (lease_until IS NULL OR lease_until < now())
                     ORDER BY next_run_at
                     LIMIT 50
                     FOR UPDATE SKIP LOCKED
                )
            RETURNING {COLUMNS}"#
        ))
        .bind(owner)
        .bind(ttl_secs as f64)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;
        rows.into_iter().map(map).collect()
    }

    async fn reschedule(
        &self,
        id: Uuid,
        next_run_at: Option<time::OffsetDateTime>,
    ) -> Result<(), AppError> {
        let mut tx = self.tx().await?;
        sqlx::query(
            "UPDATE automations
                SET next_run_at = $2, lease_until = NULL, lease_owner = NULL, updated_at = now()
              WHERE id = $1",
        )
        .bind(id)
        .bind(next_run_at)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn record_run(
        &self,
        automation_id: Uuid,
        trigger_kind: &str,
        log: &RunLog,
        status: &str,
    ) -> Result<(), AppError> {
        let mut tx = self.tx().await?;
        let steps = serde_json::to_value(&log.steps)
            .map_err(|e| AppError::Internal(format!("run log: {e}")))?;

        sqlx::query(
            r#"INSERT INTO automation_runs (id, automation_id, trigger_kind, status, log, finished_at)
               VALUES ($1, $2, $3, $4, $5, now())"#,
        )
        .bind(kernel::new_id())
        .bind(automation_id)
        .bind(trigger_kind)
        .bind(status)
        .bind(steps)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }
}

impl PgAutomation {
    /// Current values for a set of points, keyed by `(device, code)`.
    pub async fn point_values(
        &self,
        wanted: &[(Uuid, String)],
    ) -> Result<HashMap<(Uuid, String), serde_json::Value>, AppError> {
        let mut out = HashMap::new();
        if wanted.is_empty() {
            return Ok(out);
        }

        let mut tx = self.tx().await?;
        for (device_id, code) in wanted {
            let row: Option<(Option<f64>, Option<bool>, Option<String>)> = sqlx::query_as(
                "SELECT value_num, value_bool, value_text FROM device_points
                  WHERE device_id = $1 AND code = $2",
            )
            .bind(device_id)
            .bind(code)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;

            if let Some((num, boolean, text)) = row {
                let value = if let Some(b) = boolean {
                    serde_json::json!(b)
                } else if let Some(n) = num {
                    serde_json::json!(n)
                } else if let Some(t) = text {
                    serde_json::json!(t)
                } else {
                    continue;
                };
                out.insert((*device_id, code.clone()), value);
            }
        }
        tx.commit().await.map_err(db)?;
        Ok(out)
    }

    /// Everything an action needs to publish a command for `code` on `device`.
    pub async fn command_target(
        &self,
        device_id: Uuid,
        code: &str,
    ) -> Result<Option<CommandTarget>, AppError> {
        let mut tx = self.tx().await?;

        let row: Option<(Uuid, Uuid, Uuid, String)> = sqlx::query_as(
            r#"SELECT d.vendor_id, d.home_id, dp.id, dp.dp
                 FROM devices d
                 JOIN device_points dp ON dp.device_id = d.id AND dp.code = $2
                WHERE d.id = $1 AND d.home_id IS NOT NULL"#,
        )
        .bind(device_id)
        .bind(code)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(row.map(|(vendor_id, home_id, point_id, dp)| CommandTarget {
            vendor_id,
            home_id,
            point_id,
            dp,
        }))
    }

    /// Record the command an automation issued, exactly as a user command would
    /// be recorded, so the device's ack settles it through the same path.
    pub async fn enqueue_command(
        &self,
        target: &CommandTarget,
        command_id: Uuid,
        device_id: Uuid,
        payload: serde_json::Value,
    ) -> Result<(), AppError> {
        let mut tx = self.tx().await?;
        sqlx::query(
            r#"INSERT INTO commands
                 (id, vendor_id, device_id, point_id, op, payload, status,
                  requested_at, dispatched_at)
               VALUES ($1, $2, $3, $4, 'set', $5, 'queued', now(), now())"#,
        )
        .bind(command_id)
        .bind(target.vendor_id)
        .bind(device_id)
        .bind(target.point_id)
        .bind(&payload)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    /// Automation notifications go to the home's owner in v1; per-member
    /// routing is a Phase 6 concern.
    pub async fn notify_owner(
        &self,
        home_id: Uuid,
        title: &str,
        body: &str,
    ) -> Result<(), AppError> {
        let mut tx = self.tx().await?;
        let owner: Option<Uuid> = sqlx::query_scalar("SELECT owner_id FROM homes WHERE id = $1")
            .bind(home_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;

        let Some(owner) = owner else {
            return Ok(());
        };

        sqlx::query(
            r#"INSERT INTO notifications (id, user_id, kind, title, body)
               VALUES ($1, $2, 'automation', $3, $4)"#,
        )
        .bind(kernel::new_id())
        .bind(owner)
        .bind(title)
        .bind(body)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)
    }
}

#[derive(Debug, Clone)]
pub struct CommandTarget {
    pub vendor_id: Uuid,
    pub home_id: Uuid,
    pub point_id: Uuid,
    pub dp: String,
}

/// Whether a definition needs a schedule at all — a device-triggered automation
/// never sits in the due queue.
pub fn is_scheduled(def: &AutomationDef) -> bool {
    matches!(def.trigger, Trigger::Schedule { .. })
}
