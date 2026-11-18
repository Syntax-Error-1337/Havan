//! Append-only audit writes and reads.

use std::future::Future;

use domain::audit::{AuditEntry, AuditRepo, AuditRow};
use kernel::{AppError, TenantCtx};
use uuid::Uuid;

use crate::pg::Pg;

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

/// One row as PostgreSQL returns it. Named because clippy is right: a nine-tuple in a
/// `let` is a type nobody can read at the call site.
type Row = (
    Uuid,
    time::OffsetDateTime,
    String,
    Option<Uuid>,
    // The actor's email, resolved at read time: the log stores who acted, not what they were called then.
    Option<String>,
    String,
    String,
    Option<Uuid>,
    Option<String>,
    Option<String>,
    serde_json::Value,
);

pub struct PgAudit {
    pg: Pg,
}

impl PgAudit {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }
}

impl AuditRepo for PgAudit {
    async fn record(&self, entry: &AuditEntry) -> Result<(), AppError> {
        // Written in the actor's vendor context, so row-level security applies to the
        // writer as well: a bug that carries the wrong vendor id fails instead of writing
        // an entry into somebody else's trail.
        let mut tx = self
            .pg
            .scoped(&TenantCtx::vendor(entry.vendor_id, "audit-write"))
            .await?;

        sqlx::query(
            r#"INSERT INTO audit_log
                 (id, vendor_id, actor_kind, actor_id, action, target_kind, target_id,
                  request_id, ip, detail)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9::inet,$10)"#,
        )
        .bind(kernel::new_id())
        .bind(entry.vendor_id)
        .bind(entry.actor_kind.as_str())
        .bind(entry.actor_id)
        .bind(&entry.action)
        .bind(&entry.target_kind)
        .bind(entry.target_id)
        .bind(&entry.request_id)
        .bind(&entry.ip)
        .bind(&entry.detail)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)
    }

    async fn page_all(
        &self,
        vendor: Option<Uuid>,
        before: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<AuditRow>, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("audit-read-all")).await?;

        let rows: Vec<Row> = sqlx::query_as(
            r#"SELECT a.id, a.ts, a.actor_kind, a.actor_id, u.email, a.action, a.target_kind, a.target_id,
                      a.request_id, host(a.ip), a.detail
                 FROM audit_log a
                 LEFT JOIN vendor_users u ON u.id = a.actor_id AND u.vendor_id = a.vendor_id
                WHERE ($1::uuid IS NULL OR a.vendor_id = $1)
                  AND ($2::uuid IS NULL
                       OR (a.ts, a.id) < (SELECT ts, id FROM audit_log WHERE id = $2))
                ORDER BY a.ts DESC, a.id DESC
                LIMIT $3"#,
        )
        .bind(vendor)
        .bind(before)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(rows.into_iter().map(into_row).collect())
    }

    async fn page(
        &self,
        vendor_id: Uuid,
        before: Option<Uuid>,
        limit: i64,
        filter: &domain::audit::AuditFilter,
    ) -> Result<Vec<AuditRow>, AppError> {
        let mut tx = self
            .pg
            .scoped(&TenantCtx::vendor(vendor_id, "audit-read"))
            .await?;

        // Keyset on (ts, id): two entries in the same millisecond would otherwise be
        // skipped or repeated between pages.
        let rows: Vec<Row> = sqlx::query_as(
            // The join is a read-time lookup on purpose: an audit row records the actor's id, and the address belongs
            // to the account. A member who has since been removed leaves no email, which is the truth and is visible.
            r#"SELECT a.id, a.ts, a.actor_kind, a.actor_id, u.email, a.action, a.target_kind, a.target_id,
                      a.request_id, host(a.ip), a.detail
                 FROM audit_log a
                 LEFT JOIN vendor_users u ON u.id = a.actor_id AND u.vendor_id = a.vendor_id
                WHERE a.vendor_id = $1
                  AND ($2::uuid IS NULL
                       OR (a.ts, a.id) < (SELECT ts, id FROM audit_log WHERE id = $2))
                  -- Exact action, because the values are a closed set the portal lists.
                  AND ($4::text IS NULL OR a.action = $4)
                  -- Part of an address, because the person asking usually has half a memory of it.
                  AND ($5::text IS NULL OR u.email ILIKE '%' || $5 || '%')
                ORDER BY a.ts DESC, a.id DESC
                LIMIT $3"#,
        )
        .bind(vendor_id)
        .bind(before)
        .bind(limit)
        .bind(filter.action.as_deref())
        .bind(filter.actor.as_deref())
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(rows.into_iter().map(into_row).collect())
    }
}

/// One shape, two readers: a vendor's page and the operator's page are the same rows.
fn into_row(
    (
        id,
        ts,
        actor_kind,
        actor_id,
        actor_email,
        action,
        target_kind,
        target_id,
        request_id,
        ip,
        detail,
    ): Row,
) -> AuditRow {
    AuditRow {
        id,
        ts,
        actor_kind,
        actor_id,
        actor_email,
        action,
        target_kind,
        target_id,
        request_id,
        ip,
        detail,
    }
}

/// Kept for symmetry with the other repos; the trait's futures are used directly.
pub type AuditFuture<T> = std::pin::Pin<Box<dyn Future<Output = T> + Send>>;
