use std::collections::HashMap;

use crate::pg::Pg;
use domain::devices::{
    Command, CommandRepo, Device, DevicePage, DeviceRepo, Point, PointValue, Redelivery, Settled,
    encode_cursor,
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

/// A device the sweep just aged out, with the home that should hear about it.
#[derive(Debug, Clone, Copy)]
pub struct SilentDevice {
    pub device_id: Uuid,
    pub home_id: Uuid,
}

impl PgDevices {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    /// Mark devices offline once they have been silent longer than `silence_seconds`.
    ///
    /// `online` was only ever set, never cleared: a device that died without a will message —
    /// a power cut, a dead radio, a simulator that was killed — read as online for as long as the
    /// row existed. The health page caught 99 devices "online" whose newest report was four days
    /// old, which is precisely the lie this method exists to stop telling.
    ///
    /// Cross-tenant by nature (a sweep is not scoped to a home), so it runs as the system.
    ///
    /// Returns **who** was aged out, not just how many: an app holding an open socket learns that a
    /// device went quiet because somebody publishes an event, and a count cannot be published. A
    /// device with no home is marked offline and not returned — there is nobody to tell.
    /// A device's own expectation, when its product declared one.
    ///
    /// The sweep used one window for the whole fleet. A product that reports hourly was marked offline
    /// every hour and back online an hour later, which the customer sees as a device that flickers; and
    /// a product that reports every ten seconds was left looking healthy long after it had died. The
    /// device carries its product's declared interval (copied at claim), so the sweep asks each device
    /// how late it is allowed to be — never less than the platform's floor, so a chatty product does
    /// not make the fleet twitchy.
    pub async fn age_out_stale(&self, silence_seconds: i64) -> Result<Vec<SilentDevice>, AppError> {
        let mut tx = self
            .pg
            .scoped(&kernel::TenantCtx::system("presence-sweep"))
            .await?;
        let rows: Vec<(Uuid, Option<Uuid>)> = sqlx::query_as(
            "UPDATE devices SET online = false
              WHERE online
                AND (
                  last_seen_at IS NULL
                  OR last_seen_at < now() - make_interval(secs => GREATEST(
                       $1::int,
                       -- three of its own intervals, when the product declared one
                       COALESCE(report_interval_secs, 0) * 3
                     ))
                )
          RETURNING id, home_id",
        )
        .bind(silence_seconds as i32)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .filter_map(|(device_id, home_id)| {
                home_id.map(|home_id| SilentDevice { device_id, home_id })
            })
            .collect())
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
    Option<i32>,
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
    Option<time::OffsetDateTime>,
);

// No join to `products`: that table is vendor-scoped, and a customer request
// carries no vendor. The product's identity is copied onto the device at claim.
const DEVICE_COLUMNS: &str = r#"
    d.id, d.vendor_id, d.home_id, d.room_id, r.name AS room_name, d.name,
    d.product_key, d.category, d.serial, d.online, d.fw_version, d.last_seen_at,
    d.report_interval_secs
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
        report_interval_secs,
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
        report_interval_secs,
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
        updated_at,
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
            updated_at,
        },
    )
}

impl DeviceRepo for PgDevices {
    async fn list_for_home(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        search: Option<&str>,
    ) -> Result<Vec<Device>, AppError> {
        let mut tx = self.pg.scoped(&ctx_for(user_id)).await?;

        // `search` narrows in SQL rather than in the client, because the client only holds one page: a home
        // with 1,200 devices showed 200 of them, and a serial on page four "did not exist". The trigram
        // indexes from migration 0041 make `%substring%` an index probe.
        let rows = sqlx::query_as::<_, DeviceRow>(&format!(
            r#"SELECT {DEVICE_COLUMNS}
               FROM devices d
               LEFT JOIN rooms r ON r.id = d.room_id
               WHERE d.home_id = $1
                 AND ($2::text IS NULL OR d.serial ILIKE '%' || $2 || '%' OR d.name ILIKE '%' || $2 || '%')
               ORDER BY r.sort NULLS FIRST, d.name"#
        ))
        .bind(home_id)
        .bind(search)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let point_rows = sqlx::query_as::<_, PointRow>(
            r#"SELECT dp.device_id, dp.id, dp.code, dp.dp, dp.capability, dp.slot, dp.kind,
                      dp.writable, dp.unit, dp.min_val, dp.max_val,
                      dp.value_num, dp.value_bool, dp.value_text, dp.history, dp.updated_at
               FROM device_points dp
               JOIN devices d ON d.id = dp.device_id
               WHERE d.home_id = $1
                 AND ($2::text IS NULL OR d.serial ILIKE '%' || $2 || '%' OR d.name ILIKE '%' || $2 || '%')
               ORDER BY dp.code"#,
        )
        .bind(home_id)
        .bind(search)
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

    /// One page, by keyset.
    ///
    /// Keyset rather than `OFFSET`: an offset page re-walks everything before it, so the fiftieth page
    /// of a large home costs fifty pages of work, and a device claimed between two requests shifts
    /// every later row — the classic page that skips an item or shows one twice. The ordering is
    /// `(room sort, name, id)`, and `id` is there because two devices in one room can share a name.
    async fn home_presence(
        &self,
        home_id: Uuid,
        user_id: Uuid,
    ) -> Result<domain::devices::Presence, AppError> {
        let mut tx = self.pg.scoped(&ctx_for(user_id)).await?;
        // One row, three counts. The silence rule is the client's own (`max(15 min, 3 x interval)`),
        // written once here so the header cannot drift from the tiles it summarises.
        let row: (i64, i64, i64) = sqlx::query_as(
            r#"SELECT count(*),
                      count(*) FILTER (WHERE d.online)::bigint,
                      count(*) FILTER (
                          WHERE NOT d.online
                             OR d.last_seen_at IS NULL
                             OR d.last_seen_at < now() - GREATEST(
                                    interval '15 minutes',
                                    COALESCE(d.report_interval_secs, 60) * 3 * interval '1 second')
                      )::bigint
                 FROM devices d
                WHERE d.home_id = $1"#,
        )
        .bind(home_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(domain::devices::Presence {
            total: row.0,
            online: row.1,
            silent: row.2,
        })
    }

    async fn list_page(
        &self,
        home_id: Uuid,
        user_id: Uuid,
        limit: i64,
        after: Option<(i32, String, Uuid)>,
    ) -> Result<DevicePage, AppError> {
        let mut tx = self.pg.scoped(&ctx_for(user_id)).await?;

        // One extra row is the cheapest way to know whether there is a next page: asking for a count
        // of the remainder is a second scan for a boolean.
        let fetch = limit.clamp(1, 500) + 1;
        let (after_sort, after_name, after_id) = match &after {
            Some((sort, name, id)) => (Some(*sort), Some(name.clone()), Some(*id)),
            None => (None, None, None),
        };

        let mut rows = sqlx::query_as::<_, DeviceRow>(&format!(
            r#"SELECT {DEVICE_COLUMNS}
               FROM devices d
               LEFT JOIN rooms r ON r.id = d.room_id
               WHERE d.home_id = $1
                 AND ($2::int IS NULL
                      OR (COALESCE(r.sort, -1), d.name, d.id) > ($2, $3, $4))
               ORDER BY COALESCE(r.sort, -1), d.name, d.id
               LIMIT $5"#
        ))
        .bind(home_id)
        .bind(after_sort)
        .bind(after_name)
        .bind(after_id)
        .bind(fetch)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let has_more = rows.len() as i64 > limit;
        if has_more {
            rows.truncate(limit as usize);
        }
        let ids: Vec<Uuid> = rows.iter().map(|row| row.0).collect();

        // Points for this page only. Fetching the home's points and filtering in memory is exactly
        // the cost paging exists to remove.
        let point_rows = sqlx::query_as::<_, PointRow>(
            r#"SELECT dp.device_id, dp.id, dp.code, dp.dp, dp.capability, dp.slot, dp.kind,
                      dp.writable, dp.unit, dp.min_val, dp.max_val,
                      dp.value_num, dp.value_bool, dp.value_text, dp.history, dp.updated_at
               FROM device_points dp
               WHERE dp.device_id = ANY($1)
               ORDER BY dp.code"#,
        )
        .bind(&ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let total: i64 = sqlx::query_scalar(r#"SELECT count(*) FROM devices WHERE home_id = $1"#)
            .bind(home_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(db)?;

        let mut points: HashMap<Uuid, Vec<Point>> = HashMap::new();
        for row in point_rows {
            let (device_id, point) = map_point(row);
            points.entry(device_id).or_default().push(point);
        }

        let devices: Vec<Device> = rows
            .into_iter()
            .map(|row| {
                let id = row.0;
                map_device(row, points.remove(&id).unwrap_or_default())
            })
            .collect();

        let next_cursor = match (has_more, devices.last()) {
            (true, Some(last)) => {
                // The room's sort key is not part of a `Device` — it is an ordering detail, not a
                // property of the device — so it is read back for the one row the cursor names.
                let sort: i32 = sqlx::query_scalar(
                    r#"SELECT COALESCE(r.sort, -1) FROM devices d
                         LEFT JOIN rooms r ON r.id = d.room_id WHERE d.id = $1"#,
                )
                .bind(last.id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;
                Some(encode_cursor(sort, &last.name, last.id))
            }
            _ => None,
        };

        tx.commit().await.map_err(db)?;

        Ok(DevicePage {
            devices,
            next_cursor,
            total,
        })
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
                      min_val, max_val, value_num, value_bool, value_text, history, updated_at
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

    /// Release a device from its home.
    ///
    /// Runs as the system on purpose. A release *changes the row's tenancy* — the thing RLS exists to
    /// stop an ordinary writer from doing — and no policy can express it: Postgres applies a policy's
    /// USING expression to the new row as well, and "you are a member of this device's home" is not true
    /// of a row with no home. (0038 tried; 0039 reverts it with the test results in the comment.)
    ///
    /// The authorisation therefore lives where it can be tested: `DeviceService::unclaim` requires an
    /// admin of the home before calling this, and `tests/device_release.rs` proves both halves — a 204
    /// for an admin, a 403 with nothing changed for a member.
    async fn unclaim(&self, device_id: Uuid, user_id: Uuid) -> Result<(), AppError> {
        let _ = user_id;
        let mut tx = self
            .pg
            .scoped(&kernel::TenantCtx::system("device-release"))
            .await?;
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

    async fn record_release(&self, release: &domain::devices::Release) -> Result<(), AppError> {
        let mut tx = self
            .pg
            .scoped(&kernel::TenantCtx::system("device-release"))
            .await?;
        sqlx::query(
            r#"INSERT INTO device_releases (id, device_id, vendor_id, home_id, room_id, released_by, released_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
        )
        .bind(release.id)
        .bind(release.device_id)
        .bind(release.vendor_id)
        .bind(release.home_id)
        .bind(release.room_id)
        .bind(release.released_by)
        .bind(release.released_at)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }

    async fn latest_release(
        &self,
        device_id: Uuid,
    ) -> Result<Option<domain::devices::Release>, AppError> {
        let mut tx = self
            .pg
            .scoped(&kernel::TenantCtx::system("device-release"))
            .await?;
        let row = sqlx::query_as::<
            _,
            (
                Uuid,
                Uuid,
                Uuid,
                Uuid,
                Option<Uuid>,
                Uuid,
                time::OffsetDateTime,
                Option<time::OffsetDateTime>,
            ),
        >(
            r#"SELECT id, device_id, vendor_id, home_id, room_id, released_by, released_at, restored_at
                 FROM device_releases
                WHERE device_id = $1
                ORDER BY released_at DESC
                LIMIT 1"#,
        )
        .bind(device_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(row.map(|r| domain::devices::Release {
            id: r.0,
            device_id: r.1,
            vendor_id: r.2,
            home_id: r.3,
            room_id: r.4,
            released_by: r.5,
            released_at: r.6,
            restored_at: r.7,
        }))
    }

    async fn restore(
        &self,
        device_id: Uuid,
        home_id: Uuid,
        room_id: Option<Uuid>,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        let _ = user_id;
        let mut tx = self
            .pg
            .scoped(&kernel::TenantCtx::system("device-restore"))
            .await?;

        // Both writes or neither: a device back in a home whose release still reads "not undone" would offer the undo
        // again, and a second press would move it out of wherever it has been paired since.
        let restored = sqlx::query(
            r#"UPDATE devices SET home_id = $2, room_id = $3, state = 'claimed'
                WHERE id = $1 AND home_id IS NULL"#,
        )
        .bind(device_id)
        .bind(home_id)
        .bind(room_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();

        if restored == 0 {
            return Err(AppError::code(
                kernel::ErrorCode::Conflict,
                "that device is in a home already",
            ));
        }

        sqlx::query(
            r#"UPDATE device_releases SET restored_at = now()
                WHERE device_id = $1 AND restored_at IS NULL"#,
        )
        .bind(device_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;
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

    async fn insert_action_command(
        &self,
        device_id: Uuid,
        user_id: Uuid,
        action: &str,
        idempotency_key: Option<&str>,
    ) -> Result<Command, AppError> {
        let mut tx = self.pg.scoped(&ctx_for(user_id)).await?;

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
                    point_code: action.to_string(),
                    op: "action".into(),
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
               VALUES ($1, $2, $3, NULL, 'action', $4, 'queued', $5, $6, now(), now())"#,
        )
        .bind(id)
        .bind(vendor_id)
        .bind(device_id)
        .bind(serde_json::json!({ "action": action }))
        .bind(idempotency_key)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(Command {
            id,
            device_id,
            point_code: action.to_string(),
            op: "action".into(),
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
            // An action command has no point; its name is in the payload, and a status screen that answered "" for it
            // would be describing a command nobody can recognise.
            r#"SELECT c.id, c.device_id, COALESCE(dp.code, c.payload->>'action', ''), c.status
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
