use std::collections::HashMap;

use crate::pg::Pg;
use domain::devices::{
    Command, CommandRepo, Device, DeviceRepo, Point, PointValue, Redelivery, Settled,
};
use kernel::{AppError, TenantCtx};
use uuid::Uuid;

/// A refused command as PostgreSQL returns it. Named, because a six-tuple in a `let` is a
/// type nobody can read at the call site.
type RedeliveryRow = (
    Uuid,
    Uuid,
    Option<Uuid>,
    Uuid,
    Option<String>,
    Option<serde_json::Value>,
);

#[derive(Clone)]
pub struct PgDevices {
    pg: Pg,
}

impl PgDevices {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

fn ctx_for(user_id: Uuid) -> TenantCtx {
    TenantCtx::user(user_id, "devices")
}

type DeviceRow = (
    Uuid,
    Uuid,
    Option<Uuid>,
    Option<Uuid>,
    Option<String>,
    String,
    String,
    String,
    String,
    bool,
    Option<String>,
    Option<time::OffsetDateTime>,
);

type PointRow = (
    Uuid,
    Uuid,
    String,
    String,
    String,
    String,
    String,
    bool,
    Option<String>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<bool>,
    Option<String>,
    Option<String>,
);

// No join to `products`: that table is vendor-scoped, and a customer request
// carries no vendor. The product's identity is copied onto the device at claim.
const DEVICE_COLUMNS: &str = r#"
    d.id, d.vendor_id, d.home_id, d.room_id, r.name AS room_name, d.name,
    d.product_key, d.category, d.serial, d.online, d.fw_version, d.last_seen_at
"#;

fn map_device(row: DeviceRow, points: Vec<Point>) -> Device {
    let (
        id,
        vendor_id,
        home_id,
        room_id,
        room_name,
        name,
        product_key,
        category,
        serial,
        online,
        fw_version,
        last_seen_at,
    ) = row;
    Device {
        id,
        vendor_id,
        home_id,
        room_id,
        room_name,
        name,
        product_key,
        category,
        serial,
        online,
        fw_version,
        last_seen_at,
        points,
    }
}

fn map_point(row: PointRow) -> (Uuid, Point) {
    let (
        device_id,
        id,
        code,
        dp,
        capability,
        slot,
        kind,
        writable,
        unit,
        min,
        max,
        num,
        boolean,
        text,
        history,
    ) = row;

    // Exactly one of the value columns is populated by the writer.
    let value = if let Some(b) = boolean {
        Some(PointValue::Bool(b))
    } else if let Some(n) = num {
        Some(PointValue::Num(n))
    } else {
        text.map(PointValue::Text)
    };

    (
        device_id,
        Point {
            id,
            code,
            dp,
            capability,
            slot,
            kind,
            writable,
            unit,
            min,
            max,
            history,
            value,
        },
    )
}

impl DeviceRepo for PgDevices {
    async fn list_for_home(&self, home_id: Uuid, user_id: Uuid) -> Result<Vec<Device>, AppError> {
        let mut tx = self.pg.scoped(&ctx_for(user_id)).await?;

        let rows = sqlx::query_as::<_, DeviceRow>(&format!(
            r#"SELECT {DEVICE_COLUMNS}
               FROM devices d
               LEFT JOIN rooms r ON r.id = d.room_id
               WHERE d.home_id = $1
               ORDER BY r.sort NULLS FIRST, d.name"#
        ))
        .bind(home_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let point_rows = sqlx::query_as::<_, PointRow>(
            r#"SELECT dp.device_id, dp.id, dp.code, dp.dp, dp.capability, dp.slot, dp.kind,
                      dp.writable, dp.unit, dp.min_val, dp.max_val,
                      dp.value_num, dp.value_bool, dp.value_text, dp.history
               FROM device_points dp
               JOIN devices d ON d.id = dp.device_id
               WHERE d.home_id = $1
               ORDER BY dp.code"#,
        )
        .bind(home_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        let mut points: HashMap<Uuid, Vec<Point>> = HashMap::new();
        for row in point_rows {
            let (device_id, point) = map_point(row);
            points.entry(device_id).or_default().push(point);
        }

        Ok(rows
            .into_iter()
            .map(|row| {
                let id = row.0;
                map_device(row, points.remove(&id).unwrap_or_default())
            })
            .collect())
    }

    async fn get(&self, device_id: Uuid, user_id: Uuid) -> Result<Option<Device>, AppError> {
        let mut tx = self.pg.scoped(&ctx_for(user_id)).await?;

        let row = sqlx::query_as::<_, DeviceRow>(&format!(
            r#"SELECT {DEVICE_COLUMNS}
               FROM devices d
               LEFT JOIN rooms r ON r.id = d.room_id
               WHERE d.id = $1"#
        ))
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        let Some(row) = row else {
            return Ok(None);
        };

        let points = sqlx::query_as::<_, PointRow>(
            r#"SELECT device_id, id, code, dp, capability, slot, kind, writable, unit,
                      min_val, max_val, value_num, value_bool, value_text, history
               FROM device_points WHERE device_id = $1 ORDER BY code"#,
        )
        .bind(device_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?
        .into_iter()
        .map(|r| map_point(r).1)
        .collect();

        tx.commit().await.map_err(db)?;
        Ok(Some(map_device(row, points)))
    }

    async fn home_of(&self, device_id: Uuid, user_id: Uuid) -> Result<Option<Uuid>, AppError> {
        let mut tx = self.pg.scoped(&ctx_for(user_id)).await?;
        let home =
            sqlx::query_scalar::<_, Option<Uuid>>("SELECT home_id FROM devices WHERE id = $1")
                .bind(device_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .flatten();
        tx.commit().await.map_err(db)?;
        Ok(home)
    }

    async fn rename(&self, device_id: Uuid, user_id: Uuid, name: &str) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&ctx_for(user_id)).await?;
        let n = sqlx::query("UPDATE devices SET name = $2 WHERE id = $1")
            .bind(device_id)
            .bind(name)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        tx.commit().await.map_err(db)?;

        // Zero rows means RLS hid the device: report it as missing, not as a
        // permission error, so the API is not an existence oracle.
        if n == 0 {
            return Err(AppError::not_found("device"));
        }
        Ok(())
    }

    async fn assign_room(
        &self,
        device_id: Uuid,
        user_id: Uuid,
        room_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&ctx_for(user_id)).await?;

        if let Some(room_id) = room_id {
            let same_home: bool = sqlx::query_scalar(
                r#"SELECT EXISTS (
                     SELECT 1 FROM rooms r
                     JOIN devices d ON d.home_id = r.home_id
                     WHERE r.id = $1 AND d.id = $2)"#,
            )
            .bind(room_id)
            .bind(device_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;
            if !same_home {
                return Err(AppError::not_found("room"));
            }
        }

        let n = sqlx::query("UPDATE devices SET room_id = $2 WHERE id = $1")
            .bind(device_id)
            .bind(room_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        tx.commit().await.map_err(db)?;

        if n == 0 {
            return Err(AppError::not_found("device"));
        }
        Ok(())
    }

    async fn unclaim(&self, device_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&ctx_for(user_id)).await?;
        let n = sqlx::query(
            "UPDATE devices SET home_id = NULL, room_id = NULL, state = 'released' WHERE id = $1",
        )
        .bind(device_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        tx.commit().await.map_err(db)?;

        if n == 0 {
            return Err(AppError::not_found("device"));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Commands. The row is the record of intent; publishing it is the API's job.
// ---------------------------------------------------------------------------

impl CommandRepo for PgDevices {
    async fn insert_command(
        &self,
        device_id: Uuid,
        user_id: Uuid,
        point_id: Uuid,
        point_code: &str,
        payload: serde_json::Value,
        idempotency_key: Option<&str>,
    ) -> Result<Command, AppError> {
        let mut tx = self.pg.scoped(&ctx_for(user_id)).await?;

        // An idempotent replay returns the command already in flight instead of
        // dispatching a second one — the app's retry must not blink the light.
        if let Some(key) = idempotency_key {
            let existing: Option<(Uuid, String)> = sqlx::query_as(
                "SELECT id, status FROM commands WHERE device_id = $1 AND idempotency_key = $2",
            )
            .bind(device_id)
            .bind(key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;

            if let Some((id, status)) = existing {
                tx.commit().await.map_err(db)?;
                return Ok(Command {
                    id,
                    device_id,
                    point_code: point_code.to_string(),
                    op: "set".into(),
                    status,
                    created: false,
                });
            }
        }

        let vendor_id: Uuid = sqlx::query_scalar("SELECT vendor_id FROM devices WHERE id = $1")
            .bind(device_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::not_found("device"))?;

        let id = kernel::new_id();
        sqlx::query(
            r#"INSERT INTO commands
                 (id, vendor_id, device_id, point_id, op, payload, status,
                  idempotency_key, requested_by, requested_at, dispatched_at)
               VALUES ($1, $2, $3, $4, 'set', $5, 'queued', $6, $7, now(), now())"#,
        )
        .bind(id)
        .bind(vendor_id)
        .bind(device_id)
        .bind(point_id)
        .bind(&payload)
        .bind(idempotency_key)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(Command {
            id,
            device_id,
            point_code: point_code.to_string(),
            op: "set".into(),
            status: "queued".into(),
            created: true,
        })
    }

    async fn find_command(
        &self,
        command_id: Uuid,
        user_id: Uuid,
    ) -> Result<Option<Command>, AppError> {
        let mut tx = self.pg.scoped(&ctx_for(user_id)).await?;
        let row = sqlx::query_as::<_, (Uuid, Uuid, String, String)>(
            r#"SELECT c.id, c.device_id, COALESCE(dp.code, ''), c.status
               FROM commands c LEFT JOIN device_points dp ON dp.id = c.point_id
               WHERE c.id = $1"#,
        )
        .bind(command_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(row.map(|(id, device_id, point_code, status)| Command {
            id,
            device_id,
            point_code,
            op: "set".into(),
            status,
            created: false,
        }))
    }

    async fn settle_from_ack(
        &self,
        command_id: Uuid,
        ok: bool,
        reported: serde_json::Value,
        code: Option<&str>,
    ) -> Result<Option<Settled>, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("settle-ack")).await?;

        let row: Option<(Uuid, Uuid, String)> = sqlx::query_as(
            r#"UPDATE commands
                  SET status = CASE WHEN $2 THEN 'applied' ELSE 'rejected' END,
                      reported = $3,
                      error_code = $4,
                      acked_at = now()
                WHERE id = $1 AND status IN ('queued','dispatched')
            RETURNING id, device_id, status"#,
        )
        .bind(command_id)
        .bind(ok)
        .bind(&reported)
        .bind(code)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        let Some((command_id, device_id, status)) = row else {
            return Ok(None);
        };

        let home_id: Option<Uuid> = sqlx::query_scalar("SELECT home_id FROM devices WHERE id = $1")
            .bind(device_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .flatten();

        tx.commit().await.map_err(db)?;

        Ok(Some(Settled {
            command_id,
            device_id,
            home_id,
            status,
        }))
    }

    async fn fail_undelivered(&self, command_id: Uuid, reason: &str) -> Result<(), AppError> {
        // System scope: this settles one row by id on behalf of an outage, and the row's
        // own vendor is already fixed — there is no tenant to guess at.
        let mut tx = self.pg.scoped(&TenantCtx::system("command-settle")).await?;
        // `unreachable`, the status the schema already has for a command that never got
        // to a device. Not `timeout`: that word blames the hardware for our outage.
        // `attempts` counts the try that failed, so the row says "we tried once and the
        // broker was gone" rather than looking like a command nobody ever attempted.
        sqlx::query(
            r#"UPDATE commands
                  SET status = 'unreachable', error_code = $2, attempts = attempts + 1
                WHERE id = $1 AND status = 'queued'"#,
        )
        .bind(command_id)
        .bind(reason)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    async fn due_for_redelivery(
        &self,
        younger_than_secs: i64,
        limit: i64,
    ) -> Result<Vec<Redelivery>, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("redelivery")).await?;
        let rows: Vec<RedeliveryRow> = sqlx::query_as(
            r#"SELECT c.id, c.vendor_id, d.home_id, c.device_id,
                          c.payload->>'dp', c.payload->'value'
                     FROM commands c JOIN devices d ON d.id = c.device_id
                    WHERE c.status = 'unreachable'
                      AND d.home_id IS NOT NULL
                      -- Only the recent past. A command from ten minutes ago is not a
                      -- command any more: delivering it late turns a light on for no reason.
                      AND c.requested_at > now() - make_interval(secs => $1)
                    ORDER BY c.requested_at
                    LIMIT $2"#,
        )
        .bind(younger_than_secs as f64)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .filter_map(
                |(command_id, vendor_id, home_id, device_id, dp, value): RedeliveryRow| {
                    // `home_id` is filtered in SQL; re-checked because the type is Option and
                    // an unwrap here would be the one place a null takes the worker down.
                    home_id.map(|home_id| Redelivery {
                        command_id,
                        vendor_id,
                        home_id,
                        device_id,
                        dp,
                        value: value.filter(|v| !v.is_null()),
                    })
                },
            )
            .collect())
    }

    async fn claim_for_redelivery(&self, command_id: Uuid) -> Result<bool, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("redelivery")).await?;
        // One statement, conditional on the state: Postgres decides the winner, so two
        // workers cannot both get `true` no matter how they interleave.
        let claimed = sqlx::query(
            r#"UPDATE commands
                  SET status = 'dispatched', attempts = attempts + 1, dispatched_at = now()
                WHERE id = $1 AND status = 'unreachable'"#,
        )
        .bind(command_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        tx.commit().await.map_err(db)?;
        Ok(claimed == 1)
    }

    async fn release_redelivery(&self, command_id: Uuid, reason: &str) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("redelivery")).await?;
        // Back to `unreachable` so the next sweep picks it up — but the attempt is kept, so
        // a command that repeatedly fails to publish is visible rather than invisible.
        sqlx::query(
            r#"UPDATE commands SET status = 'unreachable', error_code = $2
                WHERE id = $1 AND status = 'dispatched'"#,
        )
        .bind(command_id)
        .bind(reason.chars().take(120).collect::<String>())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    async fn mark_redelivered(&self, command_id: Uuid) -> Result<bool, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("redelivery")).await?;
        let updated = sqlx::query(
            r#"UPDATE commands
                  SET status = 'dispatched', attempts = attempts + 1, dispatched_at = now()
                WHERE id = $1 AND status = 'unreachable'"#,
        )
        .bind(command_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        tx.commit().await.map_err(db)?;
        // False means another worker got there first, which is fine and worth knowing.
        Ok(updated == 1)
    }

    async fn sweep_timeouts(&self, older_than_secs: i64) -> Result<Vec<Settled>, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("sweep-timeouts")).await?;

        let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
            r#"UPDATE commands
                  SET status = 'timeout'
                WHERE status IN ('queued','dispatched')
                  AND COALESCE(dispatched_at, requested_at) < now() - make_interval(secs => $1)
            RETURNING id, device_id"#,
        )
        .bind(older_than_secs as f64)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let mut out = Vec::with_capacity(rows.len());
        for (command_id, device_id) in rows {
            let home_id: Option<Uuid> =
                sqlx::query_scalar("SELECT home_id FROM devices WHERE id = $1")
                    .bind(device_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(db)?
                    .flatten();
            out.push(Settled {
                command_id,
                device_id,
                home_id,
                status: "timeout".into(),
            });
        }

        tx.commit().await.map_err(db)?;
        Ok(out)
    }
}
