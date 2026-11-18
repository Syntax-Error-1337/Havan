use domain::privacy::{
    EXPORT_NOTIFICATION_LIMIT, EXPORT_TELEMETRY_DAYS, EXPORT_TELEMETRY_MAX_ROWS, ErasureReport,
    ExportBundle, ExportReceipt, PrivacyRepo, PurgeReport,
};
use kernel::{AppError, TenantCtx};
use serde_json::Value;
use uuid::Uuid;

use crate::pg::Pg;

/// Runs as `haven_system`: a data-subject request is about a person, not a tenant, and the person
/// may be a member of homes belonging to different vendors. That is the one legitimate reason to
/// cross tenants, and it is why every statement below is keyed by `user_id`.
#[derive(Clone)]
pub struct PgPrivacy {
    pg: Pg,
}

impl PgPrivacy {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

impl PrivacyRepo for PgPrivacy {
    async fn record_export(&self, user_id: Uuid, bytes: i64) -> Result<(), AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("dsar-export")).await?;
        sqlx::query("INSERT INTO privacy_exports (id, user_id, bytes) VALUES ($1, $2, $3)")
            .bind(kernel::new_id())
            .bind(user_id)
            .bind(bytes)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn exports(&self, user_id: Uuid) -> Result<Vec<ExportReceipt>, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("dsar-export")).await?;
        let rows = sqlx::query_as::<_, (time::OffsetDateTime, i64)>(
            "SELECT created_at, bytes FROM privacy_exports WHERE user_id = $1 ORDER BY created_at DESC LIMIT 20",
        )
        .bind(user_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .map(|(created_at, bytes)| ExportReceipt { created_at, bytes })
            .collect())
    }

    async fn export(&self, user_id: Uuid) -> Result<ExportBundle, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("dsar-export")).await?;

        let user: Option<Value> = sqlx::query_scalar(
            r#"SELECT jsonb_build_object(
                     'id', id, 'email', email, 'phone', phone, 'display_name', display_name,
                     'locale', locale, 'created_at', created_at, 'disabled_at', disabled_at)
                 FROM users WHERE id = $1"#,
        )
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        let Some(user) = user else {
            return Err(AppError::not_found("user"));
        };

        // Co-members appear as a count and a role, never as people: their email is their data.
        let homes: Vec<Value> = sqlx::query_scalar(
            r#"SELECT jsonb_build_object(
                     'id', h.id,
                     'name', h.name,
                     'timezone', h.timezone,
                     'geo', CASE WHEN h.geo_lat IS NULL THEN NULL
                                 ELSE jsonb_build_object('lat', h.geo_lat, 'lon', h.geo_lon) END,
                     'role', (SELECT role FROM home_members m
                               WHERE m.home_id = h.id AND m.user_id = $1),
                     'member_count', (SELECT count(*) FROM home_members m WHERE m.home_id = h.id),
                     'rooms', COALESCE((SELECT jsonb_agg(jsonb_build_object('id', r.id, 'name', r.name))
                                          FROM rooms r WHERE r.home_id = h.id), '[]'::jsonb),
                     'devices', COALESCE((
                        SELECT jsonb_agg(jsonb_build_object(
                                 'id', d.id, 'name', d.name, 'serial', d.serial,
                                 'product_key', d.product_key, 'online', d.online,
                                 'points', COALESCE((
                                    SELECT jsonb_object_agg(p.code, jsonb_build_object(
                                             'value', CASE p.kind
                                                        WHEN 'bool' THEN to_jsonb(p.value_bool)
                                                        WHEN 'int' THEN to_jsonb(p.value_num)
                                                        WHEN 'float' THEN to_jsonb(p.value_num)
                                                        ELSE to_jsonb(p.value_text) END,
                                             'updated_at', p.updated_at))
                                      FROM device_points p WHERE p.device_id = d.id), '{}'::jsonb)))
                          FROM devices d WHERE d.home_id = h.id), '[]'::jsonb),
                     'scenes', COALESCE((SELECT jsonb_agg(jsonb_build_object('id', s.id, 'name', s.name))
                                           FROM scenes s WHERE s.home_id = h.id), '[]'::jsonb),
                     'automations', COALESCE((SELECT jsonb_agg(jsonb_build_object(
                                              'id', a.id, 'name', a.name, 'enabled', a.enabled,
                                              'definition', a.definition))
                                                FROM automations a WHERE a.home_id = h.id), '[]'::jsonb))
                 FROM homes h
                WHERE h.owner_id = $1
                   OR EXISTS (SELECT 1 FROM home_members m WHERE m.home_id = h.id AND m.user_id = $1)
                ORDER BY h.created_at"#,
        )
        .bind(user_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let notifications: Vec<Value> = sqlx::query_scalar(
            r#"SELECT jsonb_build_object('id', id, 'kind', kind, 'title', title, 'body', body,
                                         'data', data, 'read_at', read_at, 'created_at', created_at)
                 FROM notifications WHERE user_id = $1
                ORDER BY created_at DESC LIMIT $2"#,
        )
        .bind(user_id)
        .bind(EXPORT_NOTIFICATION_LIMIT)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let telemetry: Vec<Value> = sqlx::query_scalar(
            r#"SELECT jsonb_build_object('ts', t.ts, 'device_id', t.device_id, 'point_id', t.point_id,
                                         'num', t.num, 'bool', t.bool, 'txt', t.txt)
                 FROM telemetry t
                WHERE t.device_id IN (
                        SELECT d.id FROM devices d
                         WHERE d.home_id IN (
                            SELECT h.id FROM homes h WHERE h.owner_id = $1
                            UNION
                            SELECT m.home_id FROM home_members m WHERE m.user_id = $1))
                  AND t.ts > now() - make_interval(days => $2)
                ORDER BY t.ts DESC
                LIMIT $3"#,
        )
        .bind(user_id)
        .bind(EXPORT_TELEMETRY_DAYS as i32)
        .bind(EXPORT_TELEMETRY_MAX_ROWS)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        sqlx::query(
            r#"INSERT INTO data_requests (id, user_id, kind, completed_at, detail)
               VALUES ($1, $2, 'export', now(), jsonb_build_object(
                   'homes', $3::int, 'notifications', $4::int, 'telemetry_rows', $5::int))"#,
        )
        .bind(kernel::new_id())
        .bind(user_id)
        .bind(homes.len() as i32)
        .bind(notifications.len() as i32)
        .bind(telemetry.len() as i32)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(ExportBundle {
            format_version: 1,
            exported_at: kernel::now(),
            user,
            homes,
            notifications,
            telemetry,
            notes: vec![
                format!(
                    "Telemetry is limited to the last {EXPORT_TELEMETRY_DAYS} days and {EXPORT_TELEMETRY_MAX_ROWS} rows."
                ),
                "Other members of your homes appear as a count and a role: their contact details are their own data."
                    .to_string(),
                "Device data belongs to the vendor that made the device as well as to you; this export is the copy we hold as the platform."
                    .to_string(),
            ],
        })
    }

    async fn erase(&self, user_id: Uuid) -> Result<ErasureReport, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("dsar-erase")).await?;

        let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        if exists.is_none() {
            return Err(AppError::not_found("user"));
        }

        // Read before anything is deleted: the window comes from the licence covering the devices
        // in this person's homes, and those rows are about to go. Learned the hard way — computing
        // it later silently fell back to the default for everybody.
        //
        // A person can have devices from more than one vendor, so the *longest* window wins: the
        // alternative is destroying data one vendor is still obliged to keep.
        let retention: (i32,) = sqlx::query_as(
            r#"SELECT coalesce(max(g.retention_days), 90)
                 FROM licence_grants g
                WHERE (g.expires_at IS NULL OR g.expires_at > now())
                  AND g.vendor_id IN (
                      SELECT DISTINCT d.vendor_id
                        FROM devices d
                        JOIN homes h ON h.id = d.home_id
                       WHERE h.owner_id = $1
                          OR h.id IN (SELECT home_id FROM home_members WHERE user_id = $1)
                  )"#,
        )
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        let retention_days = i64::from(retention.0);

        // A home with other people in it is not ours to delete. Erasing this account would either
        // evict them or leave them with a home whose owner no longer exists, so the request is
        // refused with the count and the operator (or the person) transfers ownership first.
        let shared: (i64,) = sqlx::query_as(
            r#"SELECT count(*) FROM homes h
                WHERE h.owner_id = $1
                  AND (SELECT count(*) FROM home_members m WHERE m.home_id = h.id) > 1"#,
        )
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;

        if shared.0 > 0 {
            let mut err = AppError::code(
                kernel::ErrorCode::Conflict,
                "you still own a home that somebody else lives in; transfer it before erasing your account",
            );
            if let AppError::Coded { details, .. } = &mut err {
                details.insert("shared_homes".into(), shared.0.to_string());
            }
            return Err(err);
        }

        // Counted before the homes go: `home_members` cascades with its home, so a delete after
        // the fact reports zero removals for memberships that were in fact removed.
        let memberships: (i64,) =
            sqlx::query_as("SELECT count(*) FROM home_members WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(db)?;

        // Devices go with the homes they were claimed into, because a device with no home is a
        // vendor's activated unit that nobody can see or control — worse than deleted.
        let devices = sqlx::query(
            "DELETE FROM devices WHERE home_id IN (SELECT id FROM homes WHERE owner_id = $1)",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();

        let homes = sqlx::query("DELETE FROM homes WHERE owner_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();

        sqlx::query("DELETE FROM home_members WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;

        let notifications = sqlx::query("DELETE FROM notifications WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();

        let sessions = sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();

        let tokens = sqlx::query("DELETE FROM push_tokens WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();

        // The row stays as a tombstone: referential integrity for audit rows, and `@invalid`
        // cannot receive mail if anything still tries. `purge_after` is set here, once, because a
        // renegotiated grant afterwards must not move the finish line for somebody who has left.
        let purge_after: (time::OffsetDateTime,) = sqlx::query_as(
            r#"UPDATE users
                  SET email = 'erased+' || id::text || '@invalid',
                      phone = NULL,
                      password_hash = NULL,
                      display_name = 'Deleted account',
                      avatar_url = NULL,
                      totp_secret = NULL,
                      disabled_at = now(),
                      purge_after = now() + make_interval(days => $2)
                WHERE id = $1
             RETURNING purge_after"#,
        )
        .bind(user_id)
        .bind(retention.0)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;

        let report = ErasureReport {
            homes_deleted: homes as i64,
            devices_deleted: devices as i64,
            memberships_removed: memberships.0,
            notifications_deleted: notifications as i64,
            sessions_revoked: sessions as i64,
            push_tokens_removed: tokens as i64,
            retention_days,
            purge_after: purge_after.0,
        };

        sqlx::query(
            r#"INSERT INTO data_requests (id, user_id, kind, completed_at, detail)
               VALUES ($1, $2, 'erase', now(), $3)"#,
        )
        .bind(kernel::new_id())
        .bind(user_id)
        .bind(serde_json::to_value(&report).unwrap_or(Value::Null))
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;
        Ok(report)
    }

    async fn purging_soon(&self) -> Result<i64, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("dsar-due")).await?;
        let (n,): (i64,) = sqlx::query_as(
            "SELECT count(*) FROM users WHERE purge_after IS NOT NULL AND purge_after <= now()",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(n)
    }

    async fn purge_due(&self, limit: i64) -> Result<PurgeReport, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("dsar-purge")).await?;

        // `purge_after <= now()` and nothing else: a tombstone that is not due is not a candidate,
        // so a run can be repeated as often as the worker likes without shortening anybody's
        // window. The order is oldest-first so a backlog drains in the order people asked.
        let purged: Vec<(Uuid,)> = sqlx::query_as(
            r#"DELETE FROM users
                WHERE id IN (
                    SELECT id FROM users
                     WHERE purge_after IS NOT NULL AND purge_after <= now()
                     ORDER BY purge_after
                     LIMIT $1
                )
                RETURNING id"#,
        )
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        // The receipt is the point of keeping anything: a person who complains about a deletion
        // gets a row that says when it happened and how long we kept it, and no way to look them
        // up again. `data_requests` has no foreign key precisely so it survives this.
        for (user_id,) in &purged {
            sqlx::query(
                r#"INSERT INTO data_requests (id, user_id, kind, completed_at, detail)
                   VALUES ($1, $2, 'purge', now(), $3)"#,
            )
            .bind(kernel::new_id())
            .bind(user_id)
            .bind(serde_json::json!({ "note": "retention window elapsed; tombstone deleted" }))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }

        tx.commit().await.map_err(db)?;

        Ok(PurgeReport {
            purged: purged.into_iter().map(|(id,)| id).collect(),
        })
    }
}
