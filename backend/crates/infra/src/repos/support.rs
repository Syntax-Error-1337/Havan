//! The support bundle: what a vendor can hand to their own support desk.
//!
//! Built by **allowlist**, not by serialising rows and redacting the bad parts. A bundle is the one
//! artefact that leaves the building — pasted into a ticket, attached to an email — so a field that
//! is in it by accident is a field disclosed by accident. Every key below is named here on purpose.
//!
//! What is never in it: who lives in the home, the home's name, its address, its coordinates, the
//! owner's identity. Those belong to the platform's customer, and a vendor troubleshooting a device
//! does not need them to see that its firmware is crashing.

use kernel::{AppError, TenantCtx};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::pg::Pg;

pub const SUPPORT_COMMANDS: i64 = 20;
pub const SUPPORT_AUDIT: i64 = 20;

#[derive(Clone)]
pub struct PgSupport {
    pg: Pg,
}

impl PgSupport {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    /// One device's bundle when `device_id` is given, otherwise a fleet summary.
    pub async fn bundle(
        &self,
        vendor_id: Uuid,
        device_id: Option<Uuid>,
    ) -> Result<Value, AppError> {
        let mut tx = self
            .pg
            .scoped(&TenantCtx {
                vendor_id: Some(vendor_id),
                actor: kernel::Actor::User {
                    user_id: Uuid::nil(),
                    vendor_id: Some(vendor_id),
                },
                request_id: "support-bundle".into(),
            })
            .await?;

        let vendor: Option<Value> = sqlx::query_scalar(
            r#"SELECT jsonb_build_object('id', id, 'slug', slug, 'name', name, 'status', status)
                 FROM vendors WHERE id = $1"#,
        )
        .bind(vendor_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        let Some(vendor) = vendor else {
            return Err(AppError::not_found("vendor"));
        };

        // The device block carries what a support engineer reads first, and nothing about the home.
        let devices: Vec<Value> = sqlx::query_scalar(
            r#"SELECT jsonb_build_object(
                     'id', d.id, 'product_key', d.product_key, 'category', d.category,
                     'name', d.name, 'serial', d.serial, 'state', d.state, 'online', d.online,
                     'firmware', d.fw_version, 'last_seen_at', d.last_seen_at,
                     'claimed_at', d.claimed_at, 'transport', d.transport,
                     'bound', d.home_id IS NOT NULL)
                 FROM devices d
                WHERE d.vendor_id = $1 AND ($2::uuid IS NULL OR d.id = $2)
                ORDER BY d.created_at DESC
                LIMIT 200"#,
        )
        .bind(vendor_id)
        .bind(device_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let commands: Vec<Value> = sqlx::query_scalar(
            r#"SELECT jsonb_build_object(
                     'id', c.id, 'device_id', c.device_id, 'op', c.op, 'status', c.status,
                     'requested_at', c.requested_at, 'acked_at', c.acked_at,
                     'dispatched_at', c.dispatched_at, 'error_code', c.error_code,
                     'attempts', c.attempts, 'payload', c.payload, 'reported', c.reported)
                 FROM commands c
                WHERE c.vendor_id = $1 AND ($2::uuid IS NULL OR c.device_id = $2)
                ORDER BY c.requested_at DESC
                LIMIT $3"#,
        )
        .bind(vendor_id)
        .bind(device_id)
        .bind(SUPPORT_COMMANDS)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let audit: Vec<Value> = sqlx::query_scalar(
            r#"SELECT jsonb_build_object('ts', ts, 'action', action, 'actor_kind', actor_kind,
                                         'target_kind', target_kind, 'target_id', target_id,
                                         'detail', detail)
                 FROM audit_log WHERE vendor_id = $1
                ORDER BY ts DESC LIMIT $2"#,
        )
        .bind(vendor_id)
        .bind(SUPPORT_AUDIT)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let licences: Value = sqlx::query_scalar(
            r#"SELECT jsonb_build_object(
                     'granted', COALESCE((SELECT sum(quantity)::bigint FROM licence_grants
                                           WHERE vendor_id = $1
                                             AND (expires_at IS NULL OR expires_at > now())), 0),
                     'used', (SELECT count(*)::bigint FROM licence_activations WHERE vendor_id = $1),
                     'remaining', COALESCE((SELECT sum(quantity)::bigint FROM licence_grants
                                             WHERE vendor_id = $1
                                               AND (expires_at IS NULL OR expires_at > now())), 0)
                                  - (SELECT count(*)::bigint FROM licence_activations WHERE vendor_id = $1))"#,
        )
        .bind(vendor_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(json!({
            "format_version": 1,
            "generated_at": kernel::now_rfc3339(),
            "platform": { "api_version": env!("CARGO_PKG_VERSION") },
            "vendor": vendor,
            "licences": licences,
            "devices": devices,
            "commands": commands,
            "recent_activity": audit,
            "notes": [
                "This bundle contains no customer identity: no home name, address, coordinates or owner.",
                "Device names are included because a device is the vendor's own product; they may be user-edited."
            ]
        }))
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}
