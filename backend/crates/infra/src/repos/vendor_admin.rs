use domain::vendor_admin::{NewVendor, VendorAdminRepo, VendorPatch, VendorRow, VendorStatus};
use kernel::{AppError, ErrorCode, TenantCtx};
use uuid::Uuid;

use crate::pg::Pg;

#[derive(Clone)]
pub struct PgVendorAdmin {
    pg: Pg,
}

impl PgVendorAdmin {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    /// The operator's own table, so it runs privileged: a vendor list cannot be scoped to a tenant
    /// that does not exist yet.
    async fn tx(&self) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, AppError> {
        self.pg.scoped(&TenantCtx::system("vendor-admin")).await
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

/// Every figure is a subquery so the list is one round trip: an operator scanning vendors does not
/// want a page that issues forty queries.
const ROW: &str = r#"
    SELECT v.id, v.slug, v.name, v.status, v.quota_devices, v.totp_required, v.created_at,
           (SELECT count(*) FROM products p WHERE p.vendor_id = v.id),
           (SELECT count(*) FROM devices d WHERE d.vendor_id = v.id),
           (SELECT coalesce(sum(g.quantity), 0)::bigint FROM licence_grants g
             WHERE g.vendor_id = v.id AND (g.expires_at IS NULL OR g.expires_at > now())),
           (SELECT count(*)::bigint FROM licence_activations a WHERE a.vendor_id = v.id)
      FROM vendors v
"#;

type Row = (
    Uuid,
    String,
    String,
    String,
    Option<i32>,
    bool,
    time::OffsetDateTime,
    i64,
    i64,
    i64,
    i64,
);

fn into_row(r: Row) -> VendorRow {
    VendorRow {
        id: r.0,
        slug: r.1,
        name: r.2,
        status: r.3,
        quota_devices: r.4,
        totp_required: r.5,
        created_at: r.6,
        products: r.7,
        devices: r.8,
        granted: r.9,
        used: r.10,
    }
}

impl VendorAdminRepo for PgVendorAdmin {
    async fn list(&self, limit: i64) -> Result<Vec<VendorRow>, AppError> {
        let mut tx = self.tx().await?;
        let sql = format!("{ROW} WHERE v.deleted_at IS NULL ORDER BY v.created_at DESC LIMIT $1");
        let rows: Vec<Row> = sqlx::query_as(&sql)
            .bind(limit.clamp(1, 200))
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(rows.into_iter().map(into_row).collect())
    }

    async fn one(&self, id: Uuid) -> Result<VendorRow, AppError> {
        let mut tx = self.tx().await?;
        let sql = format!("{ROW} WHERE v.id = $1");
        let row: Option<Row> = sqlx::query_as(&sql)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        tx.commit().await.map_err(db)?;

        row.map(into_row)
            .ok_or_else(|| AppError::code(ErrorCode::NotFound, "no such vendor"))
    }

    async fn create(&self, new: &NewVendor) -> Result<VendorRow, AppError> {
        new.validate()?;
        let mut tx = self.tx().await?;
        let id = kernel::new_id();

        let inserted: Option<(Uuid,)> = sqlx::query_as(
            r#"INSERT INTO vendors (id, slug, name, status, quota_devices)
               VALUES ($1, $2, $3, 'active', $4)
               ON CONFLICT (slug) DO NOTHING
               RETURNING id"#,
        )
        .bind(id)
        .bind(new.slug())
        .bind(new.name.trim())
        .bind(new.quota_devices)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        if inserted.is_none() {
            let mut err = AppError::code(
                ErrorCode::Conflict,
                "that slug is already taken by another vendor",
            );
            if let AppError::Coded { details, .. } = &mut err {
                details.insert("slug".into(), new.slug());
            }
            return Err(err);
        }

        // Onboarding is an operator action with money attached; it goes in the trail like one.
        sqlx::query(
            r#"INSERT INTO audit_log (id, vendor_id, actor_kind, action, target_kind, target_id, detail)
               VALUES ($1, $2, 'ops', 'vendor.created', 'vendor', $2, $3)"#,
        )
        .bind(kernel::new_id())
        .bind(id)
        .bind(serde_json::json!({ "slug": new.slug(), "quota_devices": new.quota_devices }))
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;
        self.one(id).await
    }

    async fn patch(&self, id: Uuid, patch: &VendorPatch) -> Result<VendorRow, AppError> {
        patch.validate()?;

        let mut tx = self.tx().await?;

        if let Some(status) = patch.status()? {
            // Suspension is the one transition with a written-down promise (docs/11): new
            // activations stop, existing devices keep working, the portal goes read-only. Nothing
            // here touches a device row, and that is the point.
            sqlx::query("UPDATE vendors SET status = $2 WHERE id = $1")
                .bind(id)
                .bind(status.as_str())
                .execute(&mut *tx)
                .await
                .map_err(db)?;

            // Suspension is the action somebody will be asked to justify later. No note field: the
            // reason belongs in a support ticket, not in a column we would then have to police.
            sqlx::query(
                r#"INSERT INTO audit_log (id, vendor_id, actor_kind, action, target_kind, target_id, detail)
                   VALUES ($1, $2, 'ops', $3, 'vendor', $2, '{}'::jsonb)"#,
            )
            .bind(kernel::new_id())
            .bind(id)
            .bind(match status {
                VendorStatus::Active => "vendor.reactivated",
                VendorStatus::Suspended => "vendor.suspended",
            })
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }

        if let Some(quota) = patch.quota_devices {
            sqlx::query("UPDATE vendors SET quota_devices = $2 WHERE id = $1")
                .bind(id)
                .bind(quota)
                .execute(&mut *tx)
                .await
                .map_err(db)?;

            // The ceiling is enforced at the claim path, so changing it changes who can sell.
            sqlx::query(
                r#"INSERT INTO audit_log (id, vendor_id, actor_kind, action, target_kind, target_id, detail)
                   VALUES ($1, $2, 'ops', 'vendor.quota_changed', 'vendor', $2, $3)"#,
            )
            .bind(kernel::new_id())
            .bind(id)
            .bind(serde_json::json!({ "quota_devices": quota }))
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }

        if let Some(name) = &patch.name {
            sqlx::query("UPDATE vendors SET name = $2 WHERE id = $1")
                .bind(id)
                .bind(name.trim())
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }

        tx.commit().await.map_err(db)?;
        self.one(id).await
    }
}

impl PgVendorAdmin {
    /// The status a vendor acts under, for the two places that must know it: refusing a claim, and
    /// making the portal read-only.
    pub async fn status_of(&self, vendor_id: Uuid) -> Result<String, AppError> {
        let mut tx = self.tx().await?;
        let row: Option<(String,)> =
            sqlx::query_as("SELECT status FROM vendors WHERE id = $1 AND deleted_at IS NULL")
                .bind(vendor_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        tx.commit().await.map_err(db)?;

        row.map(|(s,)| s)
            .ok_or_else(|| AppError::code(ErrorCode::Unauthorized, "no such vendor"))
    }
}
