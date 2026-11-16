use kernel::{Actor, AppError, ErrorCode, TenantCtx};
use serde::Serialize;
use uuid::Uuid;

use crate::pg::Pg;

/// What a conformance run needs to act like a device: a serial the platform expects, the secret
/// that serial will present, and a ticket a person would otherwise have to tap in the app.
#[derive(Debug, Clone, Serialize)]
pub struct ConformanceMaterial {
    pub product_key: String,
    pub product_name: String,
    pub serial: String,
    /// Shown once. It is hashed by the claim, exactly as a real device secret is.
    pub secret: String,
    pub ticket: String,
    pub home_id: Uuid,
    pub user_id: Uuid,
    pub certification: String,
    /// What the product declares about serials it has never heard of. A manifest-only product must
    /// refuse them; a trust-on-first-use product must accept and charge for them. The suite reads
    /// this rather than assuming, because both are legitimate and only one can be true at a time.
    pub allow_unknown_serial: bool,
}

/// Everything the platform recorded about a device, for the run to check itself against.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceSnapshot {
    pub id: Uuid,
    pub name: String,
    pub serial: Option<String>,
    pub online: bool,
    pub last_seen_at: Option<time::OffsetDateTime>,
    pub fw_version: Option<String>,
    pub transport: Option<String>,
    pub points: Vec<PointSnapshot>,
    pub commands: Vec<CommandSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PointSnapshot {
    pub dp: String,
    /// The vendor-facing name, so a run can issue a command without guessing what this product
    /// calls its points.
    pub code: String,
    pub kind: String,
    pub writable: bool,
    pub value_num: Option<f64>,
    pub value_bool: Option<bool>,
    pub value_text: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: time::OffsetDateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandSnapshot {
    pub op: String,
    pub status: String,
    #[serde(with = "time::serde::rfc3339::option")]
    pub acked_at: Option<time::OffsetDateTime>,
}

#[derive(Clone)]
pub struct PgConformance {
    pg: Pg,
}

impl PgConformance {
    pub fn new(pg: Pg) -> Self {
        Self { pg }
    }

    /// A conformance device lives in a home nobody else can see, owned by an account that cannot be
    /// signed into (`@haven.invalid`, no password). It is a real device on real data — that is the
    /// point — but it must never appear in a customer's app.
    async fn lab(
        &self,
        tx: &mut sqlx::PgConnection,
        vendor_id: Uuid,
    ) -> Result<(Uuid, Uuid), AppError> {
        // The whole uuid, not a slice of it: a v7 uuid's leading hex is the timestamp, so two
        // vendors created in the same millisecond would share a lab — and their devices.
        let email = format!("conformance+{vendor_id}@haven.invalid");

        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (id, email, display_name, locale)
             VALUES ($1, $2, 'Conformance lab', 'en')
             ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email
             RETURNING id",
        )
        .bind(kernel::new_id())
        .bind(&email)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;

        let home_id: Uuid = sqlx::query_scalar(
            "INSERT INTO homes (id, owner_id, name)
             SELECT $1, $2, 'Conformance lab'
             WHERE NOT EXISTS (SELECT 1 FROM homes WHERE owner_id = $2)
             RETURNING id",
        )
        .bind(kernel::new_id())
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?
        .unwrap_or_else(Uuid::nil);

        let home_id = if home_id.is_nil() {
            sqlx::query_scalar::<_, Uuid>("SELECT id FROM homes WHERE owner_id = $1 LIMIT 1")
                .bind(user_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| AppError::Database(Box::new(e)))?
        } else {
            sqlx::query(
                "INSERT INTO home_members (home_id, user_id, role) VALUES ($1, $2, 'owner')
                 ON CONFLICT DO NOTHING",
            )
            .bind(home_id)
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Database(Box::new(e)))?;
            home_id
        };

        Ok((home_id, user_id))
    }

    /// Prepare one run. A fresh serial every time, so running the suite twice does not collide with
    /// the licence check on the first run's activation — and so a re-run genuinely proves the path.
    pub async fn prepare(
        &self,
        vendor_id: Uuid,
        product_id: Uuid,
    ) -> Result<ConformanceMaterial, AppError> {
        let mut tx = self
            .pg
            .scoped(&TenantCtx {
                vendor_id: Some(vendor_id),
                actor: Actor::System {
                    job: "conformance-prepare",
                },
                request_id: "conformance-prepare".into(),
            })
            .await?;

        let product: Option<(String, String, String, bool)> = sqlx::query_as(
            "SELECT product_key, name, certification,
                    coalesce((thing_model #>> '{constraints,allow_unknown_serial}')::bool, false)
             FROM products WHERE id = $1 AND vendor_id = $2",
        )
        .bind(product_id)
        .bind(vendor_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;

        let Some((product_key, product_name, certification, allow_unknown_serial)) = product else {
            return Err(AppError::code(
                ErrorCode::NotFound,
                "no such product for this vendor",
            ));
        };

        let (home_id, user_id) = self.lab(&mut tx, vendor_id).await?;

        // A licence has to exist: a conformance device is an activated device, and pretending
        // otherwise would let a vendor certify a device they have not paid for.
        let used: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM licence_activations WHERE vendor_id = $1 AND product_id = $2",
        )
        .bind(vendor_id)
        .bind(product_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;
        let granted: i64 = sqlx::query_scalar(
            "SELECT coalesce(sum(quantity), 0) FROM licence_grants
             WHERE vendor_id = $1 AND (expires_at IS NULL OR expires_at > now())",
        )
        .bind(vendor_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;

        if used >= granted {
            let mut err = AppError::code(
                ErrorCode::LicenceExhausted,
                "no licences left: a conformance run activates a device",
            );
            if let AppError::Coded { details, .. } = &mut err {
                details.insert("granted".into(), granted.to_string());
                details.insert("used".into(), used.to_string());
            }
            return Err(err);
        }

        let serial = format!(
            "CONF-{}",
            kernel::new_id().simple().to_string()[..12].to_uppercase()
        );
        sqlx::query(
            "INSERT INTO product_serials (vendor_id, product_id, serial) VALUES ($1, $2, $3)
             ON CONFLICT (product_id, serial) DO NOTHING",
        )
        .bind(vendor_id)
        .bind(product_id)
        .bind(&serial)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(Box::new(e)))?;

        Ok(ConformanceMaterial {
            product_key,
            product_name,
            serial,
            // Two v4 UUIDs of entropy, hex, 64 characters — the same shape a factory provisioner
            // would write into a device.
            secret: format!("{}{}", kernel::new_id().simple(), Uuid::new_v4().simple()),
            ticket: String::new(),
            home_id,
            user_id,
            certification,
            allow_unknown_serial,
        })
    }

    /// The account that owns a conformance device, or a refusal.
    ///
    /// The check is on the *home's owner*: this door exists to drive the lab, and an operator token
    /// that could command a customer's bulb would be a way to make somebody's light blink from a
    /// test script. A conformance home is one owned by a `conformance+…@haven.invalid` account —
    /// deliberately not a home *named* "Conformance lab", because a name is a string the account
    /// can change and a customer's device must not become commandable by renaming a room.
    pub async fn lab_owner(&self, vendor_id: Uuid, device_id: Uuid) -> Result<Uuid, AppError> {
        let mut tx = self
            .pg
            .scoped(&TenantCtx {
                vendor_id: Some(vendor_id),
                actor: Actor::System {
                    job: "conformance-lab-owner",
                },
                request_id: "conformance-lab-owner".into(),
            })
            .await?;

        let row: Option<(String, Uuid)> = sqlx::query_as(
            "SELECT u.email, h.owner_id FROM devices d
             JOIN homes h ON h.id = d.home_id
             JOIN users u ON u.id = h.owner_id
             WHERE d.id = $1 AND d.vendor_id = $2",
        )
        .bind(device_id)
        .bind(vendor_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(Box::new(e)))?;

        match row {
            Some((email, owner))
                if email.starts_with("conformance+") && email.ends_with("@haven.invalid") =>
            {
                Ok(owner)
            }
            Some(_) => Err(AppError::code(
                ErrorCode::Forbidden,
                "this device is not in the conformance lab",
            )),
            None => Err(AppError::code(ErrorCode::NotFound, "no such device")),
        }
    }

    /// What the platform believes about a device, after the run has published.
    pub async fn snapshot(
        &self,
        vendor_id: Uuid,
        device_id: Uuid,
    ) -> Result<DeviceSnapshot, AppError> {
        let mut tx = self
            .pg
            .scoped(&TenantCtx {
                vendor_id: Some(vendor_id),
                actor: Actor::System {
                    job: "conformance-snapshot",
                },
                request_id: "conformance-snapshot".into(),
            })
            .await?;

        type DeviceRow = (
            Uuid,
            String,
            Option<String>,
            bool,
            Option<time::OffsetDateTime>,
            Option<String>,
            Option<String>,
        );

        let row: Option<DeviceRow> = sqlx::query_as(
            "SELECT id, name, serial, online, last_seen_at, fw_version, transport
                 FROM devices WHERE id = $1 AND vendor_id = $2",
        )
        .bind(device_id)
        .bind(vendor_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;

        let Some((id, name, serial, online, last_seen_at, fw_version, transport)) = row else {
            return Err(AppError::code(ErrorCode::NotFound, "no such device"));
        };

        #[allow(clippy::type_complexity)]
        let points: Vec<(
            String,
            String,
            String,
            bool,
            Option<f64>,
            Option<bool>,
            Option<String>,
            time::OffsetDateTime,
        )> = sqlx::query_as(
            "SELECT dp, code, kind, writable, value_num, value_bool, value_text, updated_at
             FROM device_points WHERE device_id = $1 ORDER BY dp",
        )
        .bind(device_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;

        let commands: Vec<(String, String, Option<time::OffsetDateTime>)> = sqlx::query_as(
            "SELECT op, status, acked_at FROM commands
             WHERE device_id = $1 ORDER BY requested_at DESC LIMIT 5",
        )
        .bind(device_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Database(Box::new(e)))?;

        tx.commit()
            .await
            .map_err(|e| AppError::Database(Box::new(e)))?;

        Ok(DeviceSnapshot {
            id,
            name,
            serial,
            online,
            last_seen_at,
            fw_version,
            transport,
            points: points
                .into_iter()
                .map(
                    |(dp, code, kind, writable, value_num, value_bool, value_text, updated_at)| {
                        PointSnapshot {
                            dp,
                            code,
                            kind,
                            writable,
                            value_num,
                            value_bool,
                            value_text,
                            updated_at,
                        }
                    },
                )
                .collect(),
            commands: commands
                .into_iter()
                .map(|(op, status, acked_at)| CommandSnapshot {
                    op,
                    status,
                    acked_at,
                })
                .collect(),
        })
    }
}
