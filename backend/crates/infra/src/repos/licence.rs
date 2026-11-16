use domain::licence::{
    ActivationRow, Invoice, InvoiceLine, LicenceRepo, LicenceSummary, NewGrant, period_bounds,
};
use kernel::{AppError, TenantCtx};
use uuid::Uuid;

use crate::pg::Pg;

/// The vendor-scoped reads and the operator grant live in one repo because they share a
/// table and a definition of "balance"; they do not share a context. Reads run as the
/// vendor (row-level security does the scoping), the grant runs as `haven_system` because
/// an operator acts on a vendor it is not.
#[derive(Clone)]
pub struct PgLicence {
    pg: Pg,
}

impl PgLicence {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    async fn vendor_tx(
        &self,
        vendor_id: Uuid,
    ) -> Result<sqlx::Transaction<'static, sqlx::Postgres>, AppError> {
        self.pg
            .scoped(&TenantCtx {
                vendor_id: Some(vendor_id),
                actor: kernel::Actor::User {
                    user_id: Uuid::nil(),
                    vendor_id: Some(vendor_id),
                },
                request_id: "licences".into(),
            })
            .await
    }
}

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(Box::new(e))
}

impl LicenceRepo for PgLicence {
    async fn summary(&self, vendor_id: Uuid) -> Result<LicenceSummary, AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;

        // Every figure is a cast of a subquery over rows in two tables; the casts are
        // explicit because `sum(bigint)` is `numeric` in Postgres and would decode as a
        // decimal we then have to parse.
        let row: (i64, i64, i64, i64, i64, Option<i32>, Option<String>) = sqlx::query_as(
            r#"SELECT
                   (SELECT COALESCE(sum(quantity), 0)::bigint
                      FROM licence_grants
                     WHERE vendor_id = $1 AND (expires_at IS NULL OR expires_at > now())),
                   (SELECT count(*)::bigint
                      FROM licence_activations WHERE vendor_id = $1),
                   (SELECT COALESCE(sum(quantity::bigint * unit_price_cents), 0)::bigint
                      FROM licence_grants
                     WHERE vendor_id = $1 AND (expires_at IS NULL OR expires_at > now())),
                   (SELECT count(*)::bigint
                      FROM licence_activations
                     WHERE vendor_id = $1
                       AND period = (date_trunc('month', now()))::date),
                   (SELECT COALESCE(sum(unit_price_cents), 0)::bigint
                      FROM licence_activations WHERE vendor_id = $1),
                   (SELECT unit_price_cents
                      FROM licence_grants
                     WHERE vendor_id = $1 AND (expires_at IS NULL OR expires_at > now())
                     ORDER BY granted_at DESC LIMIT 1),
                   (SELECT currency
                      FROM licence_grants
                     WHERE vendor_id = $1 AND (expires_at IS NULL OR expires_at > now())
                     ORDER BY granted_at DESC LIMIT 1)"#,
        )
        .bind(vendor_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        let (granted, used, granted_value, this_period, used_value, price, currency) = row;
        Ok(LicenceSummary {
            granted,
            used,
            // Not clamped: a grant revoked after devices were live leaves this negative,
            // and an operator should see that rather than a tidy zero.
            remaining: granted - used,
            activated_this_period: this_period,
            granted_value_cents: granted_value,
            used_value_cents: used_value,
            unit_price_cents: price.unwrap_or(0),
            currency: currency.unwrap_or_else(|| "USD".into()),
        })
    }

    async fn activations(
        &self,
        vendor_id: Uuid,
        limit: i64,
    ) -> Result<Vec<ActivationRow>, AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;
        let rows: Vec<(String, String, String, time::OffsetDateTime, i32)> = sqlx::query_as(
            r#"SELECT a.serial, p.product_key, p.name, a.activated_at, a.unit_price_cents
                 FROM licence_activations a
                 JOIN products p ON p.id = a.product_id
                WHERE a.vendor_id = $1
                ORDER BY a.activated_at DESC
                LIMIT $2"#,
        )
        .bind(vendor_id)
        .bind(limit.clamp(1, 500))
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(rows
            .into_iter()
            .map(
                |(serial, product_key, product_name, activated_at, unit_price_cents)| {
                    ActivationRow {
                        serial,
                        product_key,
                        product_name,
                        activated_at,
                        unit_price_cents,
                    }
                },
            )
            .collect())
    }

    async fn grant(&self, grant: &NewGrant) -> Result<Uuid, AppError> {
        grant.validate()?;
        let mut tx = self.pg.scoped(&TenantCtx::system("licence-grant")).await?;

        let id = kernel::new_id();
        sqlx::query(
            r#"INSERT INTO licence_grants
                   (id, vendor_id, quantity, unit_price_cents, currency, expires_at, note)
               VALUES ($1, $2, $3, $4, $5,
                       CASE WHEN $6::bigint IS NULL THEN NULL
                            ELSE now() + ($6::text || ' days')::interval END,
                       $7)"#,
        )
        .bind(id)
        .bind(grant.vendor_id)
        .bind(grant.quantity as i32)
        .bind(grant.unit_price_cents)
        .bind(grant.currency())
        .bind(grant.expires_in_days)
        .bind(grant.note.as_deref())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(id)
    }

    async fn invoice_preview(&self, vendor_id: Uuid, period: &str) -> Result<Invoice, AppError> {
        let (start, end) = period_bounds(period)?;
        let mut tx = self.vendor_tx(vendor_id).await?;

        // A frozen document wins over the live query: if this month has been issued, the answer is
        // what we sent, not what the ledger says now.
        let issued: Option<(time::OffsetDateTime, String, i32, i64)> = sqlx::query_as(
            "SELECT issued_at, currency, quantity, amount_cents
               FROM licence_invoices WHERE vendor_id = $1 AND period = $2",
        )
        .bind(vendor_id)
        .bind(period)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        let lines: Vec<(String, String, i32, time::OffsetDateTime)> = if issued.is_some() {
            sqlx::query_as(
                r#"SELECT l.serial, l.product_key, l.unit_price_cents, l.activated_at
                     FROM licence_invoice_lines l
                     JOIN licence_invoices i ON i.id = l.invoice_id
                    WHERE i.vendor_id = $1 AND i.period = $2
                    ORDER BY l.activated_at"#,
            )
            .bind(vendor_id)
            .bind(period)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?
        } else {
            sqlx::query_as(
                r#"SELECT a.serial, p.product_key, a.unit_price_cents, a.activated_at
                     FROM licence_activations a
                     JOIN products p ON p.id = a.product_id
                    WHERE a.vendor_id = $1 AND a.activated_at >= $2 AND a.activated_at < $3
                    ORDER BY a.activated_at"#,
            )
            .bind(vendor_id)
            .bind(start)
            .bind(end)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?
        };

        tx.commit().await.map_err(db)?;

        let lines: Vec<InvoiceLine> = lines
            .into_iter()
            .map(
                |(serial, product_key, unit_price_cents, activated_at)| InvoiceLine {
                    serial,
                    product_key,
                    unit_price_cents,
                    activated_at,
                },
            )
            .collect();

        let (status, currency, quantity, amount_cents, issued_at) = match issued {
            Some((issued_at, currency, quantity, amount_cents)) => (
                "issued",
                currency,
                i64::from(quantity),
                amount_cents,
                Some(issued_at),
            ),
            None => (
                "draft",
                self.currency_for(vendor_id).await?,
                lines.len() as i64,
                lines.iter().map(|l| i64::from(l.unit_price_cents)).sum(),
                None,
            ),
        };

        Ok(Invoice {
            period: period.to_string(),
            currency,
            quantity,
            amount_cents,
            status,
            issued_at,
            lines,
        })
    }

    async fn invoice_issue(
        &self,
        vendor_id: Uuid,
        period: &str,
        issued_by: Option<Uuid>,
    ) -> Result<Invoice, AppError> {
        let (start, end) = period_bounds(period)?;

        // A future month cannot be invoiced: the devices have not been activated yet, and a
        // document for them would be an invoice for an intention.
        let now = time::OffsetDateTime::now_utc();
        if start > now {
            return Err(AppError::validation(
                "this period has not started yet; there is nothing to invoice",
            ));
        }

        let currency = self.currency_for(vendor_id).await?;
        let mut tx = self
            .pg
            .scoped(&TenantCtx::system("licence-invoice"))
            .await?;

        let rows: Vec<(String, String, i32, time::OffsetDateTime)> = sqlx::query_as(
            r#"SELECT a.serial, p.product_key, a.unit_price_cents, a.activated_at
                 FROM licence_activations a
                 JOIN products p ON p.id = a.product_id
                WHERE a.vendor_id = $1 AND a.activated_at >= $2 AND a.activated_at < $3
                ORDER BY a.activated_at"#,
        )
        .bind(vendor_id)
        .bind(start)
        .bind(end)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;

        let quantity = rows.len() as i32;
        let amount_cents: i64 = rows.iter().map(|(_, _, price, _)| i64::from(*price)).sum();
        let id = kernel::new_id();

        // `ON CONFLICT DO NOTHING`: a second issue of the same month — a re-run, a retry, two
        // operators clicking at once — keeps the first document instead of making a second bill.
        let inserted: Option<(Uuid,)> = sqlx::query_as(
            r#"INSERT INTO licence_invoices
                   (id, vendor_id, period, currency, quantity, amount_cents, issued_by)
               VALUES ($1, $2, $3, $4, $5, $6, $7)
               ON CONFLICT (vendor_id, period) DO NOTHING
               RETURNING id"#,
        )
        .bind(id)
        .bind(vendor_id)
        .bind(period)
        .bind(&currency)
        .bind(quantity)
        .bind(amount_cents)
        .bind(issued_by)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        if inserted.is_none() {
            tx.commit().await.map_err(db)?;
            return self.invoice_preview(vendor_id, period).await;
        }

        for (serial, product_key, unit_price_cents, activated_at) in &rows {
            sqlx::query(
                r#"INSERT INTO licence_invoice_lines
                       (invoice_id, vendor_id, serial, product_key, unit_price_cents, activated_at)
                   VALUES ($1, $2, $3, $4, $5, $6)"#,
            )
            .bind(id)
            .bind(vendor_id)
            .bind(serial)
            .bind(product_key)
            .bind(unit_price_cents)
            .bind(activated_at)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }

        sqlx::query(
            r#"INSERT INTO audit_log (id, vendor_id, actor_kind, actor_id, action, target_kind, target_id, detail)
               VALUES ($1, $2, 'ops', $3, 'licence.invoiced', 'licence_invoice', $4, $5)"#,
        )
        .bind(kernel::new_id())
        .bind(vendor_id)
        .bind(issued_by)
        .bind(id)
        .bind(serde_json::json!({
            "period": period,
            "quantity": quantity,
            "amount_cents": amount_cents,
            "currency": currency,
        }))
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        tx.commit().await.map_err(db)?;

        self.invoice_preview(vendor_id, period).await
    }

    async fn invoices(&self, vendor_id: Uuid, limit: i64) -> Result<Vec<Invoice>, AppError> {
        let mut tx = self.vendor_tx(vendor_id).await?;
        let periods: Vec<(String,)> = sqlx::query_as(
            "SELECT period FROM licence_invoices WHERE vendor_id = $1 ORDER BY period DESC LIMIT $2",
        )
        .bind(vendor_id)
        .bind(limit.clamp(1, 60))
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        let mut out = Vec::with_capacity(periods.len());
        for (period,) in periods {
            out.push(self.invoice_preview(vendor_id, &period).await?);
        }
        Ok(out)
    }
}

impl PgLicence {
    /// What this vendor's activations are priced in. The newest grant wins, because a vendor that
    /// renegotiated in a new currency bills the new one from then on.
    async fn currency_for(&self, vendor_id: Uuid) -> Result<String, AppError> {
        let mut tx = self
            .pg
            .scoped(&TenantCtx::system("licence-currency"))
            .await?;
        let currency: Option<(String,)> = sqlx::query_as(
            "SELECT currency FROM licence_grants WHERE vendor_id = $1
              ORDER BY granted_at DESC LIMIT 1",
        )
        .bind(vendor_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;

        Ok(currency.map(|(c,)| c).unwrap_or_else(|| "USD".to_string()))
    }
}
