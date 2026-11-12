//! Ingest: a device report becomes current values, telemetry, presence and an
//! event for every open app.
//!
//! This path has no user, so it runs system-scoped. It is deliberately the only
//! writer of `device_points.value_*` and `telemetry`: one place to reason about
//! ordering, one place to test.

use std::collections::HashMap;

use crate::pg::Pg;
use kernel::{AppError, TenantCtx};
use protocol::capability::History;
use protocol::envelope::{DpValue, EventMsg, ShadowMsg};
use serde::Serialize;
use uuid::Uuid;

/// One row of `device_points` as ingest needs it: identity, storage policy, and
/// the value it currently holds (so `change` can mean *change*).
#[derive(Debug, sqlx::FromRow)]
struct PointRow {
    id: Uuid,
    code: String,
    dp: String,
    kind: String,
    history: String,
    value_num: Option<f64>,
    value_bool: Option<bool>,
    value_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PointMeta {
    pub id: Uuid,
    pub code: String,
    pub dp: String,
    pub kind: String,
    pub history: History,
}

#[derive(Debug, Clone, Serialize)]
pub struct PointUpdate {
    pub device_id: Uuid,
    pub code: String,
    pub value: DpValue,
    pub ts: i64,
    /// False when the report repeated the stored value. Automations are edge
    /// triggered: "when it turns on", not "while it is on".
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct IngestOutcome {
    pub device_id: Uuid,
    pub home_id: Option<Uuid>,
    pub online: bool,
    pub updated: Vec<PointUpdate>,
    pub dropped_points: usize,
}

pub struct PgIngest {
    pg: Pg,
}

impl PgIngest {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    async fn tx(&self) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, AppError> {
        self.pg.scoped(&TenantCtx::system("ingest")).await
    }

    /// Apply a state report. Unknown datapoints are counted and dropped: one
    /// misbehaving device must not stall the lane for the rest of the fleet.
    pub async fn apply_event(
        &self,
        device_id: Uuid,
        msg: &EventMsg,
    ) -> Result<IngestOutcome, AppError> {
        self.apply(device_id, msg.ts, None, &msg.pts).await
    }

    /// Apply a retained shadow, which also carries presence.
    pub async fn apply_shadow(
        &self,
        device_id: Uuid,
        msg: &ShadowMsg,
    ) -> Result<IngestOutcome, AppError> {
        self.apply(device_id, msg.ts, Some(msg.online), &msg.pts)
            .await
    }

    async fn apply(
        &self,
        device_id: Uuid,
        ts: i64,
        online: Option<bool>,
        pts: &std::collections::BTreeMap<String, DpValue>,
    ) -> Result<IngestOutcome, AppError> {
        let mut tx = self.tx().await?;

        let row: Option<(Option<Uuid>, Uuid)> =
            sqlx::query_as("SELECT home_id, vendor_id FROM devices WHERE id = $1")
                .bind(device_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;

        let Some((home_id, vendor_id)) = row else {
            return Err(AppError::not_found("device"));
        };

        // shortcut: one metadata query per report. Upgrade to a per-device cache
        // when the fleet passes a few hundred reports/second per worker.
        let metas: Vec<PointRow> = sqlx::query_as(
            "SELECT id, code, dp, kind, history, value_num, value_bool, value_text
               FROM device_points WHERE device_id = $1",
        )
        .bind(device_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let by_dp: HashMap<&str, &PointRow> = metas.iter().map(|m| (m.dp.as_str(), m)).collect();

        let at = time::OffsetDateTime::from_unix_timestamp(ts).unwrap_or_else(|_| kernel::now());

        let mut updated = Vec::with_capacity(pts.len());
        let mut dropped = 0;

        for (dp, value) in pts {
            let Some(point) = by_dp.get(dp.as_str()).copied() else {
                dropped += 1;
                continue;
            };

            let (num, boolean, text) = split_value(&point.kind, value);
            let unchanged = point.value_num == num
                && point.value_bool == boolean
                && point.value_text.as_deref() == text.as_deref();

            sqlx::query(
                r#"UPDATE device_points
                     SET value_num = $2, value_bool = $3, value_text = $4, updated_at = now()
                   WHERE id = $1"#,
            )
            .bind(point.id)
            .bind(num)
            .bind(boolean)
            .bind(text.as_deref())
            .execute(&mut *tx)
            .await
            .map_err(db)?;

            // `change` means a row on a change, not on every report: a device
            // that re-reports its whole shadow every 30 s must not cost 2 880
            // rows a day per point.
            let policy = History::parse(&point.history);
            let store = match policy {
                History::Off => false,
                History::Change => !unchanged,
                History::All | History::Interval(_) => true,
            };
            if store {
                // `interval:N` is a throttle, not a flag: at most one row per point
                // per N seconds. A temperature sensor that reports every second was
                // writing 86 400 rows a day under a policy that promised 288 — the
                // policy existed and did nothing, which is worse than not having it.
                let throttle: Option<i32> = match policy {
                    History::Interval(secs) => Some(secs.min(i32::MAX as u32) as i32),
                    _ => None,
                };

                sqlx::query(
                    r#"INSERT INTO telemetry (ts, device_id, point_id, vendor_id, num, bool, txt)
                       SELECT $1, $2, $3, $4, $5, $6, $7
                        WHERE $8::integer IS NULL
                           OR NOT EXISTS (
                                SELECT 1 FROM telemetry
                                 WHERE device_id = $2 AND point_id = $3
                                   AND ts > now() - make_interval(secs => $8)
                              )"#,
                )
                .bind(at)
                .bind(device_id)
                .bind(point.id)
                .bind(vendor_id)
                .bind(num)
                .bind(boolean)
                .bind(text.as_deref())
                .bind(throttle)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            }

            updated.push(PointUpdate {
                device_id,
                code: point.code.clone(),
                value: value.clone(),
                ts,
                changed: !unchanged,
            });
        }

        let online = online.unwrap_or(true);
        sqlx::query(r#"UPDATE devices SET online = $2, last_seen_at = now() WHERE id = $1"#)
            .bind(device_id)
            .bind(online)
            .execute(&mut *tx)
            .await
            .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(IngestOutcome {
            device_id,
            home_id,
            online,
            updated,
            dropped_points: dropped,
        })
    }
}

/// One value column is written; the others are cleared so a kind change cannot
/// leave a stale value behind.
fn split_value(kind: &str, value: &DpValue) -> (Option<f64>, Option<bool>, Option<String>) {
    match kind {
        "bool" => (None, value.as_bool(), None),
        "text" | "enum" | "raw" => (None, None, value.as_text().map(str::to_string)),
        _ => (value.as_f64(), None, None),
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_land_in_the_column_their_kind_names() {
        let (num, boolean, text) = split_value("bool", &DpValue::Bool(true));
        assert_eq!((num, boolean, text), (None, Some(true), None));

        let (num, _, _) = split_value("int", &DpValue::Int(80));
        assert_eq!(num, Some(80.0));

        let (_, _, text) = split_value("text", &DpValue::Text("#ff0000".into()));
        assert_eq!(text.as_deref(), Some("#ff0000"));
    }

    #[test]
    fn history_policy_round_trips_through_storage() {
        for h in [
            History::Off,
            History::Change,
            History::All,
            History::Interval(300),
        ] {
            assert_eq!(History::parse(&h.as_str()), h);
        }
    }
}
