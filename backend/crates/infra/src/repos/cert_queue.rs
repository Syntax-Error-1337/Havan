use kernel::{AppError, TenantCtx};
use serde::Serialize;
use uuid::Uuid;

use crate::pg::Pg;

/// A product waiting for our sign-off.
#[derive(Debug, Clone, Serialize)]
pub struct QueueItem {
    pub product_id: Uuid,
    pub vendor_id: Uuid,
    pub vendor_name: String,
    pub product_key: String,
    pub name: String,
    pub category: String,
    /// When the vendor asked. Older first: the queue is a promise about order.
    #[serde(with = "time::serde::rfc3339::option")]
    pub submitted_at: Option<time::OffsetDateTime>,
    pub note: Option<String>,
}

#[derive(Clone)]
pub struct PgCertificationQueue {
    pg: Pg,
}

impl PgCertificationQueue {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    /// Everything submitted and not yet decided, oldest first.
    ///
    /// Cross-vendor on purpose, which is why it lives behind the ops door: a vendor must never see
    /// another vendor's product, let alone the fact that it exists.
    pub async fn pending(&self, limit: i64) -> Result<Vec<QueueItem>, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("cert-queue")).await?;
        type Row = (
            Uuid,
            Uuid,
            String,
            String,
            String,
            String,
            Option<time::OffsetDateTime>,
            Option<String>,
        );

        let rows: Vec<Row> = sqlx::query_as(
            r#"SELECT p.id, p.vendor_id, v.name, p.product_key, p.name, p.category,
                      p.certified_at, p.certification_note
                 FROM products p
                 JOIN vendors v ON v.id = p.vendor_id
                WHERE p.certification = 'submitted' AND v.deleted_at IS NULL
                ORDER BY p.certified_at NULLS FIRST, p.created_at
                LIMIT $1"#,
        )
        .bind(limit.clamp(1, 200))
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(Box::new(e)))?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    product_id,
                    vendor_id,
                    vendor_name,
                    product_key,
                    name,
                    category,
                    submitted_at,
                    note,
                )| {
                    QueueItem {
                        product_id,
                        vendor_id,
                        vendor_name,
                        product_key,
                        name,
                        category,
                        submitted_at,
                        note,
                    }
                },
            )
            .collect())
    }

    /// Cross-vendor counts for the console's first screen: how many tenants, how many are suspended,
    /// what is waiting for review.
    pub async fn overview(&self) -> Result<Overview, AppError> {
        let mut tx = self.pg.scoped(&TenantCtx::system("admin-overview")).await?;
        let row: (i64, i64, i64, i64) = sqlx::query_as(
            r#"SELECT (SELECT count(*) FROM vendors WHERE deleted_at IS NULL),
                      (SELECT count(*) FROM vendors WHERE status = 'suspended' AND deleted_at IS NULL),
                      (SELECT count(*) FROM products WHERE certification = 'submitted'),
                      (SELECT count(*) FROM licence_invoices
                        WHERE date_trunc('month', issued_at) = date_trunc('month', now()))"#,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;
        tx.commit()
            .await
            .map_err(|e| AppError::Database(Box::new(e)))?;

        Ok(Overview {
            vendors: row.0,
            suspended: row.1,
            awaiting_certification: row.2,
            invoices_this_month: row.3,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub vendors: i64,
    pub suspended: i64,
    pub awaiting_certification: i64,
    pub invoices_this_month: i64,
}
