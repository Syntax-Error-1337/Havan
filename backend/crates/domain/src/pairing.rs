//! Pairing: the ticket a phone mints, and the claim a device makes with it.
//!
//! Two credentials meet here. The *ticket* proves a person with rights to a home
//! asked for this pairing; the *device secret* proves the hardware is the
//! hardware. Neither alone is enough, which is why a photographed QR code or a
//! leaked factory log cannot put a device into someone's home.

use std::sync::Arc;

use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use kernel::{AppError, ErrorCode};
use protocol::thing_model::{ResolvedPoint, ThingModel};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const TICKET_TTL_SECS: i64 = 300;
/// A retried claim inside this window returns the same device instead of a 409.
pub const CLAIM_IDEMPOTENCY_SECS: i64 = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketClaims {
    pub jti: Uuid,
    pub home_id: Uuid,
    pub user_id: Uuid,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Clone)]
pub struct ProductRecord {
    pub id: Uuid,
    pub vendor_id: Uuid,
    pub thing_model: ThingModel,
}

#[derive(Debug, Clone)]
pub struct DeviceRecord {
    pub id: Uuid,
    pub secret_hash: String,
    pub state: String,
    pub home_id: Option<Uuid>,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ClaimRequest {
    pub product_key: String,
    pub serial: String,
    pub secret: String,
    pub ticket: String,
    pub fw: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClaimedDevice {
    pub device_id: Uuid,
    pub home_id: Uuid,
    pub name: String,
    pub points: usize,
}

/// Storage the slice needs.
pub trait PairingRepo {
    fn mint_ticket(
        &self,
        claims: &TicketClaims,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn ticket(&self, jti: Uuid)
    -> impl Future<Output = Result<Option<TicketRow>, AppError>> + Send;

    fn product_by_key(
        &self,
        product_key: &str,
    ) -> impl Future<Output = Result<Option<ProductRecord>, AppError>> + Send;

    fn device_by_serial(
        &self,
        product_id: Uuid,
        serial: &str,
    ) -> impl Future<Output = Result<Option<DeviceRecord>, AppError>> + Send;

    /// Atomically: put the device in the home, write its points from the model,
    /// and burn the ticket. Returns the device id and how many points landed.
    fn claim(
        &self,
        plan: ClaimPlan,
    ) -> impl Future<Output = Result<(Uuid, usize), AppError>> + Send;
}

#[derive(Debug, Clone)]
pub struct TicketRow {
    pub jti: Uuid,
    pub home_id: Uuid,
    pub user_id: Uuid,
    pub expires_at: time::OffsetDateTime,
    pub consumed_at: Option<time::OffsetDateTime>,
    pub device_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct ClaimPlan {
    pub product_id: Uuid,
    pub vendor_id: Uuid,
    /// Copied onto the device so the app never joins the vendor-scoped table.
    pub product_key: String,
    pub category: String,
    pub home_id: Uuid,
    pub jti: Uuid,
    pub serial: String,
    pub secret_hash: String,
    pub name: String,
    pub fw: Option<String>,
    pub points: Vec<ResolvedPoint>,
}

pub struct PairingService<R> {
    repo: Arc<R>,
    signing: SigningKey,
    verifying: VerifyingKey,
}

impl<R: PairingRepo + Send + Sync> PairingService<R> {
    /// The ticket key is derived from the deployment secret, so a ticket minted
    /// by one instance verifies on another without sharing a key file.
    pub fn new(repo: Arc<R>, ticket_secret: &str) -> Self {
        let digest = Sha256::digest(ticket_secret.as_bytes());
        let signing = SigningKey::from_bytes(digest.as_slice().try_into().expect("32 bytes"));
        let verifying = signing.verifying_key();
        Self {
            repo,
            signing,
            verifying,
        }
    }

    /// Mint a ticket for one device in one home, valid for five minutes.
    /// Authorisation is the caller's: the API layer has already checked the
    /// actor's role and `can_pair`.
    pub async fn mint(&self, home_id: Uuid, user_id: Uuid, jti: Uuid) -> Result<String, AppError> {
        let now = kernel::now();
        let claims = TicketClaims {
            jti,
            home_id,
            user_id,
            exp: (now + time::Duration::seconds(TICKET_TTL_SECS)).unix_timestamp(),
            iat: now.unix_timestamp(),
        };

        self.repo.mint_ticket(&claims).await?;

        let payload = serde_json::to_vec(&claims)
            .map_err(|e| AppError::Internal(format!("ticket encoding: {e}")))?;
        let signature = self.signing.sign(&payload);

        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        Ok(format!(
            "hv1.{}.{}",
            b64.encode(&payload),
            b64.encode(signature.to_bytes())
        ))
    }

    pub fn verify_ticket(&self, ticket: &str) -> Result<TicketClaims, AppError> {
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let rest = ticket
            .strip_prefix("hv1.")
            .ok_or_else(|| bad_ticket("unsupported ticket version"))?;
        let (payload_b64, sig_b64) = rest
            .split_once('.')
            .ok_or_else(|| bad_ticket("malformed ticket"))?;

        let payload = b64
            .decode(payload_b64)
            .map_err(|_| bad_ticket("malformed ticket"))?;
        let sig_bytes: [u8; 64] = b64
            .decode(sig_b64)
            .map_err(|_| bad_ticket("malformed ticket"))?
            .try_into()
            .map_err(|_| bad_ticket("malformed ticket"))?;

        self.verifying
            .verify(&payload, &Signature::from_bytes(&sig_bytes))
            .map_err(|_| bad_ticket("ticket signature does not verify"))?;

        let claims: TicketClaims =
            serde_json::from_slice(&payload).map_err(|_| bad_ticket("malformed ticket"))?;

        if claims.exp < kernel::now().unix_timestamp() {
            return Err(AppError::code(
                ErrorCode::BadRequest,
                "ticket_expired: this pairing ticket has expired, ask the app for a new one",
            ));
        }
        Ok(claims)
    }

    /// The device-side half of pairing.
    pub async fn claim(&self, req: ClaimRequest) -> Result<ClaimedDevice, AppError> {
        let claims = self.verify_ticket(&req.ticket)?;

        let product = self
            .repo
            .product_by_key(&req.product_key)
            .await?
            .ok_or_else(|| AppError::code(ErrorCode::NotFound, "unknown_product"))?;

        // A product may be published without a manufacturing manifest; then the
        // first claim with a valid ticket creates the row (trust on first use).
        let allow_unknown = product.thing_model.constraints.allow_unknown_serial;

        let existing = self.repo.device_by_serial(product.id, &req.serial).await?;

        let (secret_hash, device_name) = match &existing {
            Some(device) => {
                if !crate::identity::verify_password(&req.secret, &device.secret_hash) {
                    return Err(AppError::code(
                        ErrorCode::Unauthorized,
                        "bad_secret: this device's secret does not match",
                    ));
                }
                if let Some(home_id) = device.home_id
                    && home_id != claims.home_id
                {
                    // Re-pairing after a factory reset is a separate, deliberate
                    // operation; silently stealing a device is not.
                    return Err(AppError::code(
                        ErrorCode::AlreadyClaimed,
                        "already_claimed: this device belongs to another home",
                    ));
                }
                (
                    device.secret_hash.clone(),
                    device
                        .name
                        .clone()
                        .unwrap_or_else(|| default_name(&product)),
                )
            }
            None if allow_unknown => (
                crate::identity::hash_password(&req.secret)?,
                default_name(&product),
            ),
            None => {
                return Err(AppError::code(
                    ErrorCode::NotFound,
                    "unknown_serial: this serial is not in the vendor's manifest",
                ));
            }
        };

        // The ticket must be this home's, unused, and not expired.
        let ticket = self
            .repo
            .ticket(claims.jti)
            .await?
            .ok_or_else(|| bad_ticket("ticket is not known to this deployment"))?;

        if ticket.home_id != claims.home_id {
            return Err(AppError::code(
                ErrorCode::ValidationFailed,
                "ticket_home_mismatch",
            ));
        }

        if let Some(consumed) = ticket.consumed_at {
            let age = kernel::now() - consumed;
            let same_device =
                ticket.device_id.is_some() && ticket.device_id == existing.as_ref().map(|d| d.id);
            if same_device && age.whole_seconds() <= CLAIM_IDEMPOTENCY_SECS {
                // A flaky Wi-Fi link retrying must not create a second device.
                let device_id = ticket.device_id.expect("checked above");
                return Ok(ClaimedDevice {
                    device_id,
                    home_id: claims.home_id,
                    name: device_name,
                    points: product.thing_model.resolve_points().len(),
                });
            }
            return Err(AppError::code(
                ErrorCode::Conflict,
                "ticket_used: this ticket has already paired a device",
            ));
        }

        let points = product.thing_model.resolve_points();
        if points.is_empty() {
            return Err(AppError::code(
                ErrorCode::ValidationFailed,
                "product_has_no_points",
            ));
        }

        let (device_id, written) = self
            .repo
            .claim(ClaimPlan {
                product_id: product.id,
                vendor_id: product.vendor_id,
                product_key: product.thing_model.product_key.clone(),
                category: product.thing_model.category.clone(),
                home_id: claims.home_id,
                jti: claims.jti,
                serial: req.serial.clone(),
                secret_hash,
                name: device_name.clone(),
                fw: req.fw,
                points,
            })
            .await?;

        Ok(ClaimedDevice {
            device_id,
            home_id: claims.home_id,
            name: device_name,
            points: written,
        })
    }
}

fn default_name(product: &ProductRecord) -> String {
    product.thing_model.name.clone()
}

fn bad_ticket(message: &str) -> AppError {
    AppError::code(ErrorCode::BadRequest, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        tickets: Mutex<Vec<TicketRow>>,
        device: Mutex<Option<DeviceRecord>>,
        claims: Mutex<Vec<ClaimPlan>>,
    }

    impl PairingRepo for Fake {
        async fn mint_ticket(&self, c: &TicketClaims) -> Result<(), AppError> {
            self.tickets.lock().unwrap().push(TicketRow {
                jti: c.jti,
                home_id: c.home_id,
                user_id: c.user_id,
                expires_at: kernel::now() + time::Duration::seconds(TICKET_TTL_SECS),
                consumed_at: None,
                device_id: None,
            });
            Ok(())
        }
        async fn ticket(&self, jti: Uuid) -> Result<Option<TicketRow>, AppError> {
            Ok(self
                .tickets
                .lock()
                .unwrap()
                .iter()
                .find(|t| t.jti == jti)
                .cloned())
        }
        async fn product_by_key(&self, key: &str) -> Result<Option<ProductRecord>, AppError> {
            if key != "hv_test_bulb" {
                return Ok(None);
            }
            let model: ThingModel = serde_json::from_value(serde_json::json!({
                "product_key": "hv_test_bulb",
                "name": "Test Bulb",
                "category": "light",
                "capabilities": [
                    { "name": "switch", "points": { "power": { "dp": "1", "kind": "bool" } } }
                ],
                "constraints": { "allow_unknown_serial": true }
            }))
            .unwrap();
            Ok(Some(ProductRecord {
                id: Uuid::from_u128(7),
                vendor_id: Uuid::from_u128(8),
                thing_model: model,
            }))
        }
        async fn device_by_serial(
            &self,
            _product_id: Uuid,
            _serial: &str,
        ) -> Result<Option<DeviceRecord>, AppError> {
            Ok(self.device.lock().unwrap().clone())
        }
        async fn claim(&self, plan: ClaimPlan) -> Result<(Uuid, usize), AppError> {
            let id = Uuid::from_u128(9);
            let n = plan.points.len();
            *self.device.lock().unwrap() = Some(DeviceRecord {
                id,
                secret_hash: plan.secret_hash.clone(),
                state: "claimed".into(),
                home_id: Some(plan.home_id),
                name: Some(plan.name.clone()),
            });
            let jti = plan.jti;
            self.claims.lock().unwrap().push(plan);
            for t in self.tickets.lock().unwrap().iter_mut() {
                if t.jti == jti {
                    t.consumed_at = Some(kernel::now());
                    t.device_id = Some(id);
                }
            }
            Ok((id, n))
        }
    }

    fn svc() -> PairingService<Fake> {
        PairingService::new(
            Arc::new(Fake::default()),
            "test-ticket-secret-32-bytes-long",
        )
    }

    fn request(svc: &PairingService<Fake>, home: Uuid) -> ClaimRequest {
        let jti = kernel::new_id();
        svc.repo.tickets.lock().unwrap().push(TicketRow {
            jti,
            home_id: home,
            user_id: Uuid::new_v4(),
            expires_at: kernel::now() + time::Duration::seconds(60),
            consumed_at: None,
            device_id: None,
        });
        let ticket = {
            let claims = TicketClaims {
                jti,
                home_id: home,
                user_id: Uuid::new_v4(),
                exp: (kernel::now() + time::Duration::seconds(60)).unix_timestamp(),
                iat: kernel::now().unix_timestamp(),
            };
            let payload = serde_json::to_vec(&claims).unwrap();
            let sig = svc.signing.sign(&payload);
            let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
            format!(
                "hv1.{}.{}",
                b64.encode(&payload),
                b64.encode(sig.to_bytes())
            )
        };
        ClaimRequest {
            product_key: "hv_test_bulb".into(),
            serial: "SN-1".into(),
            secret: "device-secret-value".into(),
            ticket,
            fw: Some("1.0.0".into()),
        }
    }

    #[tokio::test]
    async fn a_valid_claim_creates_points_from_the_model() {
        let svc = svc();
        let home = Uuid::new_v4();
        let req = request(&svc, home);
        let out = svc.claim(req).await.unwrap();
        assert_eq!(out.points, 1);
        assert_eq!(out.home_id, home);
    }

    #[tokio::test]
    async fn a_tampered_ticket_is_refused() {
        let svc = svc();
        let home = Uuid::new_v4();
        let mut req = request(&svc, home);
        // Flip a character in the signature.
        let last = req.ticket.pop().unwrap();
        req.ticket.push(if last == 'A' { 'B' } else { 'A' });
        assert!(svc.claim(req).await.is_err());
    }

    #[tokio::test]
    async fn an_expired_ticket_is_refused_with_a_clear_code() {
        let svc = svc();
        let claims = TicketClaims {
            jti: kernel::new_id(),
            home_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            exp: 1,
            iat: 1,
        };
        let payload = serde_json::to_vec(&claims).unwrap();
        let sig = svc.signing.sign(&payload);
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        let ticket = format!(
            "hv1.{}.{}",
            b64.encode(&payload),
            b64.encode(sig.to_bytes())
        );
        let err = svc
            .claim(ClaimRequest {
                product_key: "hv_test_bulb".into(),
                serial: "SN-1".into(),
                secret: "x".into(),
                ticket,
                fw: None,
            })
            .await
            .unwrap_err();
        assert!(err.public_message().contains("ticket_expired"));
    }

    #[tokio::test]
    async fn a_ticket_for_another_home_is_refused() {
        let svc = svc();
        let signed_for = Uuid::new_v4();
        let req = request(&svc, signed_for);

        // The row says a different home than the signed payload.
        for t in svc.repo.tickets.lock().unwrap().iter_mut() {
            t.home_id = Uuid::new_v4();
        }
        let err = svc.claim(req).await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);
    }

    #[tokio::test]
    async fn retrying_the_same_claim_returns_the_same_device() {
        // The Wi-Fi dropped after the cloud committed; the device retries.
        let svc = svc();
        let home = Uuid::new_v4();
        let req = request(&svc, home);
        let first = svc.claim(req.clone()).await.unwrap();
        let again = svc.claim(req).await.unwrap();

        assert_eq!(first.device_id, again.device_id);
        assert_eq!(svc.repo.claims.lock().unwrap().len(), 1, "no second device");
    }

    #[tokio::test]
    async fn an_unknown_product_is_a_404() {
        let svc = svc();
        let mut req = request(&svc, Uuid::new_v4());
        req.product_key = "hv_nope".into();
        let err = svc.claim(req).await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::NotFound);
    }
}
