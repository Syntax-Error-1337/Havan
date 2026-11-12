//! The vendor's own fleet, read from the same tables the customer app writes.
//!
//! Every query is scoped by `vendor_id` twice: once in the WHERE clause and once by
//! row-level security. That is deliberate duplication — the policy is the guarantee,
//! and the predicate is what makes the guarantee legible to anyone reading the SQL.

use domain::fleet::{
    Counts, FirmwareCount, FleetDevice, FleetFilter, FleetRepo, FleetStats, NewDevices,
    ProductCount,
};
use kernel::{AppError, TenantCtx};
use uuid::Uuid;

use crate::pg::Pg;

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

pub struct PgFleet {
    pg: Pg,
}

impl PgFleet {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    /// Every read opens a transaction that names the vendor.
    ///
    /// This is not decoration: `devices` carries a row-level policy keyed on
    /// `haven_vendor()`, and a query on a bare pool would return **zero rows** — which
    /// looks exactly like a vendor with no devices. The first version of this file did
    /// that, and the fleet page was convincingly empty.
    async fn tx(
        &self,
        vendor_id: Uuid,
    ) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, AppError> {
        self.pg
            .scoped(&TenantCtx::vendor(vendor_id, "fleet-read"))
            .await
    }
}

type Row = (
    Uuid,
    String,
    String,
    String,
    String,
    String,
    bool,
    bool,
    Option<String>,
    Option<time::OffsetDateTime>,
    time::OffsetDateTime,
);

impl FleetRepo for PgFleet {
    async fn devices(
        &self,
        vendor_id: Uuid,
        filter: &FleetFilter,
    ) -> Result<Vec<FleetDevice>, AppError> {
        // One extra row tells the service whether a next page exists, without a
        // second count query over a table that grows with every claim.
        let limit = filter.limit() + 1;
        let cursor = filter
            .cursor
            .as_deref()
            .and_then(|c| Uuid::parse_str(c).ok());

        let mut tx = self.tx(vendor_id).await?;
        let rows: Vec<Row> = sqlx::query_as(
            r#"SELECT d.id, d.product_key, p.name, d.name, d.serial, d.state,
                      d.home_id IS NOT NULL AS bound, d.online,
                      d.fw_version, d.last_seen_at, d.created_at
                 FROM devices d
                 JOIN products p ON p.id = d.product_id
                WHERE d.vendor_id = $1
                  AND ($2::text IS NULL OR d.product_key = $2)
                  AND ($3::text IS NULL
                       OR ($3 = 'online'  AND d.online)
                       OR ($3 = 'offline' AND NOT d.online)
                       OR ($3 = 'bound'   AND d.home_id IS NOT NULL)
                       OR ($3 = 'unbound' AND d.home_id IS NULL))
                  AND ($4::uuid IS NULL
                       OR (d.created_at, d.id) < (SELECT created_at, id FROM devices WHERE id = $4))
                ORDER BY d.created_at DESC, d.id DESC
                LIMIT $5"#,
        )
        .bind(vendor_id)
        .bind(filter.product_key.as_deref())
        .bind(filter.state.as_deref())
        .bind(cursor)
        .bind(limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    id,
                    product_key,
                    product_name,
                    name,
                    serial,
                    state,
                    bound,
                    online,
                    fw_version,
                    last_seen_at,
                    created_at,
                )| FleetDevice {
                    id,
                    product_key,
                    product_name,
                    name,
                    serial,
                    state,
                    bound,
                    online,
                    fw_version,
                    last_seen_at,
                    created_at,
                },
            )
            .collect())
    }

    async fn stats(&self, vendor_id: Uuid) -> Result<FleetStats, AppError> {
        let mut tx = self.tx(vendor_id).await?;
        let (total, online, bound, last_7d, last_30d): (i64, i64, i64, i64, i64) = sqlx::query_as(
            r#"SELECT count(*),
                          count(*) FILTER (WHERE online),
                          count(*) FILTER (WHERE home_id IS NOT NULL),
                          count(*) FILTER (WHERE created_at > now() - interval '7 days'),
                          count(*) FILTER (WHERE created_at > now() - interval '30 days')
                     FROM devices WHERE vendor_id = $1"#,
        )
        .bind(vendor_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;

        let by_product: Vec<(String, String, i64, i64)> = sqlx::query_as(
            r#"SELECT d.product_key, p.name, count(*), count(*) FILTER (WHERE d.online)
                 FROM devices d JOIN products p ON p.id = d.product_id
                WHERE d.vendor_id = $1
                GROUP BY d.product_key, p.name
                ORDER BY count(*) DESC"#,
        )
        .bind(vendor_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let firmware: Vec<(Option<String>, i64)> = sqlx::query_as(
            r#"SELECT fw_version, count(*) FROM devices
                WHERE vendor_id = $1 GROUP BY fw_version ORDER BY count(*) DESC"#,
        )
        .bind(vendor_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        Ok(FleetStats {
            devices: Counts {
                total,
                online,
                bound,
                unbound: total - bound,
            },
            new_devices: NewDevices { last_7d, last_30d },
            by_product: by_product
                .into_iter()
                .map(|(product_key, name, total, online)| ProductCount {
                    product_key,
                    name,
                    total,
                    online,
                })
                .collect(),
            firmware: firmware
                .into_iter()
                .map(|(version, devices)| FirmwareCount { version, devices })
                .collect(),
        })
    }
}
