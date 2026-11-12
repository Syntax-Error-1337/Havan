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

/// A point's three value columns, which `change` compares as one unit.
type Value = (Option<f64>, Option<bool>, Option<String>);

/// One report, as the batching path needs it: a timestamp, whether it also carries
/// presence, and the datapoints.
#[derive(Debug, Clone)]
pub struct Report {
    pub ts: i64,
    pub online: Option<bool>,
    pub pts: std::collections::BTreeMap<String, DpValue>,
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

    /// Apply many reports for one device in **one transaction**.
    ///
    /// The single-report path cost one transaction, one metadata query and two statements
    /// per point, which measured out at ~150 messages/second per worker and shed the rest
    /// at the broker (docs/08-PERFORMANCE.md). Batching removes the per-message commit and
    /// collapses the point updates to the value each point ended on; telemetry still gets a
    /// row per changed report, because that is the history, not a cache.
    pub async fn apply_reports(
        &self,
        device_id: Uuid,
        reports: &[Report],
    ) -> Result<IngestOutcome, AppError> {
        let Some(last) = reports.last() else {
            return Err(AppError::validation("no reports to apply"));
        };
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

        // Once for the batch, not once per message.
        let metas: Vec<PointRow> = sqlx::query_as(
            "SELECT id, code, dp, kind, history, value_num, value_bool, value_text
               FROM device_points WHERE device_id = $1",
        )
        .bind(device_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        let by_dp: HashMap<&str, &PointRow> = metas.iter().map(|m| (m.dp.as_str(), m)).collect();

        // The value each point starts the batch on: `change` means change since the last
        // report, which may be one earlier in this same batch.
        let mut current: HashMap<String, Value> = metas
            .iter()
            .map(|m| {
                (
                    m.dp.clone(),
                    (m.value_num, m.value_bool, m.value_text.clone()),
                )
            })
            .collect();

        let mut updated: Vec<PointUpdate> = Vec::new();
        let mut touched: HashMap<String, usize> = HashMap::new();
        let mut dropped = 0usize;

        let mut rows_ts: Vec<time::OffsetDateTime> = Vec::new();
        let mut rows_point: Vec<Uuid> = Vec::new();
        let mut rows_num: Vec<Option<f64>> = Vec::new();
        let mut rows_bool: Vec<Option<bool>> = Vec::new();
        let mut rows_text: Vec<Option<String>> = Vec::new();

        for report in reports {
            let at = time::OffsetDateTime::from_unix_timestamp(report.ts)
                .unwrap_or_else(|_| kernel::now());

            for (dp, value) in &report.pts {
                let Some(point) = by_dp.get(dp.as_str()).copied() else {
                    dropped += 1;
                    continue;
                };

                let (num, boolean, text) = split_value(&point.kind, value);
                let unchanged = current.get(dp.as_str()) == Some(&(num, boolean, text.clone()));

                let policy = History::parse(&point.history);
                let store = match policy {
                    History::Off => false,
                    History::Change => !unchanged,
                    // `interval:N` keeps the per-row guard below: the throttle is measured
                    // against what is already stored, which a batch cannot know.
                    History::All | History::Interval(_) => true,
                };

                if store {
                    match policy {
                        History::Interval(secs) => {
                            let throttle = secs.min(i32::MAX as u32) as i32;
                            sqlx::query(
                                r#"INSERT INTO telemetry (ts, device_id, point_id, vendor_id, num, bool, txt)
                                   SELECT $1, $2, $3, $4, $5, $6, $7
                                    WHERE NOT EXISTS (
                                          SELECT 1 FROM telemetry
                                           WHERE device_id = $2 AND point_id = $3
                                             AND ts > now() - make_interval(secs => $8))"#,
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
                        _ => {
                            rows_ts.push(at);
                            rows_point.push(point.id);
                            rows_num.push(num);
                            rows_bool.push(boolean);
                            rows_text.push(text.clone());
                        }
                    }
                }

                current.insert(dp.clone(), (num, boolean, text));

                // One fan-out per point per batch: apps want the state the device ended on,
                // not fifty notifications of a dimmer sweeping through its range.
                match touched.get(dp.as_str()) {
                    Some(index) => {
                        let entry = &mut updated[*index];
                        entry.value = value.clone();
                        entry.ts = report.ts;
                        entry.changed |= !unchanged;
                    }
                    None => {
                        touched.insert(dp.clone(), updated.len());
                        updated.push(PointUpdate {
                            device_id,
                            code: point.code.clone(),
                            value: value.clone(),
                            ts: report.ts,
                            changed: !unchanged,
                        });
                    }
                }
            }
        }

        // The value each point ended on, in one statement per point rather than one per
        // report — a device reporting its whole shadow every second does not need the same
        // UPDATE fifty times.
        for dp in touched.keys() {
            let (num, boolean, text) = current[dp].clone();
            sqlx::query(
                r#"UPDATE device_points
                      SET value_num = $2, value_bool = $3, value_text = $4, updated_at = now()
                    WHERE device_id = $1 AND dp = $5"#,
            )
            .bind(device_id)
            .bind(num)
            .bind(boolean)
            .bind(text.as_deref())
            .bind(dp)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }

        if !rows_ts.is_empty() {
            sqlx::query(
                r#"INSERT INTO telemetry (ts, device_id, point_id, vendor_id, num, bool, txt)
                   SELECT * FROM UNNEST(
                       $1::timestamptz[], $2::uuid[], $3::uuid[], $4::uuid[],
                       $5::float8[], $6::bool[], $7::text[])"#,
            )
            .bind(&rows_ts)
            .bind(vec![device_id; rows_ts.len()])
            .bind(&rows_point)
            .bind(vec![vendor_id; rows_ts.len()])
            .bind(&rows_num)
            .bind(&rows_bool)
            .bind(&rows_text)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }

        let online = last.online.unwrap_or(true);
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
        let report = Report {
            ts,
            online,
            pts: pts.clone(),
        };
        self.apply_reports(device_id, std::slice::from_ref(&report))
            .await
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
