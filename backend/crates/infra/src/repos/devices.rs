use std::collections::HashMap;

use crate::pg::Pg;
use domain::devices::{Device, DeviceRepo, Point, PointValue};
use kernel::{AppError, TenantCtx};
use uuid::Uuid;

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
    Option<Uuid>,
    Option<Uuid>,
    Option<String>,
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
    bool,
    Option<String>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<bool>,
    Option<String>,
);

// No join to `products`: that table is vendor-scoped, and a customer request
// carries no vendor. The product's identity is copied onto the device at claim.
const DEVICE_COLUMNS: &str = r#"
    d.id, d.home_id, d.room_id, r.name AS room_name, d.name,
    d.product_key, d.category, d.online, d.fw_version, d.last_seen_at
"#;

fn map_device(row: DeviceRow, points: Vec<Point>) -> Device {
    let (
        id,
        home_id,
        room_id,
        room_name,
        name,
        product_key,
        category,
        online,
        fw_version,
        last_seen_at,
    ) = row;
    Device {
        id,
        home_id,
        room_id,
        room_name,
        name,
        product_key,
        category,
        online,
        fw_version,
        last_seen_at,
        points,
    }
}

fn map_point(row: PointRow) -> (Uuid, Point) {
    let (device_id, id, code, capability, slot, kind, writable, unit, min, max, num, boolean, text) =
        row;

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
            capability,
            slot,
            kind,
            writable,
            unit,
            min,
            max,
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
            r#"SELECT dp.device_id, dp.id, dp.code, dp.capability, dp.slot, dp.kind,
                      dp.writable, dp.unit, dp.min_val, dp.max_val,
                      dp.value_num, dp.value_bool, dp.value_text
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
            r#"SELECT device_id, id, code, capability, slot, kind, writable, unit,
                      min_val, max_val, value_num, value_bool, value_text
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
