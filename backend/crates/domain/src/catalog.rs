//! The catalog: what a vendor sells, described as data.
//!
//! A product is only useful once it is *published*, and publishing is gated by
//! the six thing-model rules in `protocol::thing_model`. That gate is the whole
//! reason this slice exists: a broken model produces a device the app cannot
//! draw, and no amount of client code fixes it.

use std::sync::Arc;

use kernel::{AppError, ErrorCode};
use protocol::thing_model::{RuleViolation, ThingModel};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: Uuid,
    pub vendor_id: Uuid,
    pub product_key: String,
    pub name: String,
    pub category: String,
    pub status: String,
    /// `uncertified | submitted | certified | revoked`. Publishing is the vendor saying it is
    /// done; this is the operator saying it was tested. Only a certified product can be
    /// claimed in the app.
    pub certification: String,
    pub schema_version: i32,
    pub thing_model: ThingModel,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
}

/// Every method carries the vendor: the repository opens a transaction scoped to
/// that tenant, so RLS — not this code — decides what a vendor may see.
/// What the radar needs to draw a device it just heard: a name, a category and
/// an icon, keyed by the hash the advertisement carries.
#[derive(Debug, Clone, Serialize)]
pub struct PublicProduct {
    pub product_key: String,
    pub name: String,
    pub category: String,
    pub icon: Option<String>,
    pub hash: String,
}

pub trait CatalogRepo {
    fn insert(
        &self,
        vendor_id: Uuid,
        product: &Product,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn list(&self, vendor_id: Uuid) -> impl Future<Output = Result<Vec<Product>, AppError>> + Send;

    fn get(
        &self,
        vendor_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<Product>, AppError>> + Send;

    fn update(
        &self,
        vendor_id: Uuid,
        product: &Product,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Remove a product nobody depends on.
    ///
    /// Returns `Some(Unused { devices, activations })` when it refused, so the refusal can name what
    /// is in the way; `None` when the row is gone.
    fn delete(
        &self,
        vendor_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<Unused>, AppError>> + Send;

    fn publish(
        &self,
        vendor_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Move a product's certification, but only along a legal edge of the state machine.
    ///
    /// `to` is the state being asked for; the repository checks the current state, so two
    /// operators clicking certify at once cannot produce a half-certified product, and a
    /// `revoked` product cannot be quietly re-certified without a new submission.
    /// `vendor_id: None` is an operator acting across tenants, which is a different
    /// privilege from a vendor acting on its own product — an argument, not an inference, so
    /// the caller has to say which door it came through.
    fn set_certification(
        &self,
        vendor_id: Option<Uuid>,
        id: Uuid,
        to: &str,
        note: Option<&str>,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Published products by advertisement hash or key. Cross-tenant by design:
    /// a published product is public, and a radar does not know which vendor it
    /// is looking at.
    fn public_lookup(
        &self,
        terms: &[String],
    ) -> impl Future<Output = Result<Vec<PublicProduct>, AppError>> + Send;
}

/// The certification state machine, as data.
///
/// A product earns its place in our app in steps, and every step is somebody's decision:
/// the vendor says "ready" (submit), we say "tested" (certify), and we can take it back
/// (revoke) without pretending the vendor never submitted it. Re-submitting after a revoke
/// is the only way back in, which is what makes a revoke mean something.
pub const CERTIFICATION_STATES: [&str; 4] = ["uncertified", "submitted", "certified", "revoked"];

/// Is `from -> to` a legal certification move? Same-state is a no-op, so a retried POST is
/// not an error the caller has to interpret.
/// Why a product could not be removed: something already points at it.
///
/// Present as data rather than as a formatted error inside the repo, because the two numbers are
/// what make the refusal useful — "6 devices are live on this product" tells the vendor what to do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Unused {
    pub devices: i64,
    pub activations: i64,
}

pub fn certification_transition(from: &str, to: &str) -> Result<(), AppError> {
    if !CERTIFICATION_STATES.contains(&to) {
        return Err(AppError::validation(format!(
            "unknown certification state: {to}"
        )));
    }
    if from == to {
        return Ok(());
    }

    let legal = matches!(
        (from, to),
        ("uncertified", "submitted")
            | ("submitted", "uncertified")
            | ("submitted", "certified")
            | ("certified", "revoked")
            | ("revoked", "submitted")
    );

    if legal {
        Ok(())
    } else {
        Err(AppError::code(
            kernel::ErrorCode::Conflict,
            format!("certification cannot go from {from} to {to}"),
        ))
    }
}

pub struct CatalogService<R> {
    repo: Arc<R>,
}

impl<R: CatalogRepo + Send + Sync> CatalogService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    /// Drafts may be invalid — that is what a draft is for. Publishing is the
    /// gate, and it cannot be bypassed.
    pub async fn create(
        &self,
        vendor_id: Uuid,
        model: ThingModel,
        publish_now: bool,
    ) -> Result<Product, AppError> {
        validate_identity(&model)?;
        if publish_now {
            validate_rules(&model)?;
        }

        let product = Product {
            id: kernel::new_id(),
            vendor_id,
            product_key: model.product_key.clone(),
            name: model.name.clone(),
            category: model.category.clone(),
            status: if publish_now { "published" } else { "draft" }.to_string(),
            // Never certified on creation, however it was created: certification is a
            // decision about a tested device, and nothing is tested at insert time.
            certification: "uncertified".to_string(),
            schema_version: i32::from(model.schema_version),
            thing_model: model,
            created_at: kernel::now(),
        };

        self.repo.insert(vendor_id, &product).await?;
        Ok(product)
    }

    /// Move a product through certification. The repo checks the current state under a
    /// row lock, so this is safe against two operators clicking at once.
    pub async fn set_certification(
        &self,
        vendor_id: Option<Uuid>,
        id: Uuid,
        to: &str,
        note: Option<&str>,
    ) -> Result<(), AppError> {
        if !CERTIFICATION_STATES.contains(&to) {
            return Err(AppError::validation(format!(
                "unknown certification state: {to}"
            )));
        }
        self.repo.set_certification(vendor_id, id, to, note).await
    }

    pub async fn list(&self, vendor_id: Uuid) -> Result<Vec<Product>, AppError> {
        self.repo.list(vendor_id).await
    }

    pub async fn get(&self, vendor_id: Uuid, id: Uuid) -> Result<Product, AppError> {
        self.repo
            .get(vendor_id, id)
            .await?
            .ok_or_else(|| AppError::not_found("product"))
    }

    pub async fn update(
        &self,
        vendor_id: Uuid,
        id: Uuid,
        model: ThingModel,
    ) -> Result<Product, AppError> {
        let existing = self.get(vendor_id, id).await?;
        if existing.status == "published" {
            return Err(AppError::code(
                ErrorCode::Conflict,
                "a published product is immutable; publish a new product_key instead",
            ));
        }

        validate_identity(&model)?;
        // The key is the device's identity; changing it would orphan every
        // device already claimed under the old one.
        if model.product_key != existing.product_key {
            return Err(AppError::validation("product_key cannot change"));
        }

        let updated = Product {
            name: model.name.clone(),
            category: model.category.clone(),
            schema_version: i32::from(model.schema_version),
            thing_model: model,
            ..existing
        };
        self.repo.update(vendor_id, &updated).await?;
        Ok(updated)
    }

    /// Returns the violations instead of failing, so the portal can render them
    /// next to the field that caused them.
    /// Delete a product, or say precisely why it has to stay.
    ///
    /// Two things make a catalogue entry permanent, and both are money or hardware:
    ///
    /// - **devices** — a product with devices in people's homes cannot vanish from the store;
    /// - **licence activations** — they are the billing record. The foreign key cascades, so a
    ///   delete would erase the very rows an invoice was built from. A vendor tidying their
    ///   catalogue must not be able to delete their own revenue history.
    pub async fn delete(&self, vendor_id: Uuid, id: Uuid) -> Result<(), AppError> {
        let Some(unused) = self.repo.delete(vendor_id, id).await? else {
            return Ok(());
        };
        if unused.devices > 0 {
            return Err(AppError::code(
                kernel::ErrorCode::Conflict,
                format!(
                    "{} device(s) are live on this product; a catalogue entry that people already \
                     own cannot be removed",
                    unused.devices
                ),
            ));
        }
        if unused.activations > 0 {
            return Err(AppError::code(
                kernel::ErrorCode::Conflict,
                format!(
                    "{} licence activation(s) are recorded against this product; they are the \
                     billing history and are not deleted with the catalogue entry",
                    unused.activations
                ),
            ));
        }
        // Neither count blocked it and the row is still there: only reachable if the delete raced
        // with another, and a second answer is better than a wrong one.
        Err(AppError::not_found("product"))
    }

    pub async fn validate(
        &self,
        vendor_id: Uuid,
        id: Uuid,
    ) -> Result<Vec<RuleViolation>, AppError> {
        let product = self.get(vendor_id, id).await?;
        Ok(product.thing_model.validate().err().unwrap_or_default())
    }

    pub async fn public_lookup(&self, terms: &[String]) -> Result<Vec<PublicProduct>, AppError> {
        if terms.is_empty() || terms.len() > 50 {
            return Err(AppError::validation(
                "ask for between 1 and 50 products at a time",
            ));
        }
        self.repo.public_lookup(terms).await
    }

    pub async fn publish(&self, vendor_id: Uuid, id: Uuid) -> Result<Product, AppError> {
        let product = self.get(vendor_id, id).await?;
        if product.status == "published" {
            return Ok(product);
        }

        validate_rules(&product.thing_model)?;
        self.repo.publish(vendor_id, id).await?;
        Ok(Product {
            status: "published".into(),
            ..product
        })
    }
}

fn validate_identity(model: &ThingModel) -> Result<(), AppError> {
    let key = model.product_key.trim();
    if key.len() < 4 || key.len() > 64 {
        return Err(AppError::validation("product_key must be 4–64 characters"));
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(AppError::validation(
            "product_key may contain only lowercase letters, digits and _",
        ));
    }
    if model.name.trim().is_empty() {
        return Err(AppError::validation("a product needs a name"));
    }
    Ok(())
}

/// One error with every rule that failed, so a vendor fixes them in one pass
/// instead of one publish attempt per typo.
fn validate_rules(model: &ThingModel) -> Result<(), AppError> {
    match model.validate() {
        Ok(()) => Ok(()),
        Err(violations) => {
            let mut err = AppError::validation(format!(
                "thing model failed validation ({} problem{})",
                violations.len(),
                if violations.len() == 1 { "" } else { "s" }
            ));
            for v in violations {
                err = err.with_detail(
                    format!("rule_{}", v.rule),
                    format!("{}: {}", v.path, v.message),
                );
            }
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        products: Mutex<Vec<Product>>,
    }

    impl CatalogRepo for Fake {
        async fn insert(&self, _vendor: Uuid, product: &Product) -> Result<(), AppError> {
            self.products.lock().unwrap().push(product.clone());
            Ok(())
        }
        async fn list(&self, _vendor: Uuid) -> Result<Vec<Product>, AppError> {
            Ok(self.products.lock().unwrap().clone())
        }
        async fn get(&self, _vendor: Uuid, id: Uuid) -> Result<Option<Product>, AppError> {
            Ok(self
                .products
                .lock()
                .unwrap()
                .iter()
                .find(|p| p.id == id)
                .cloned())
        }
        async fn update(&self, _vendor: Uuid, product: &Product) -> Result<(), AppError> {
            let mut all = self.products.lock().unwrap();
            if let Some(p) = all.iter_mut().find(|p| p.id == product.id) {
                *p = product.clone();
            }
            Ok(())
        }
        async fn public_lookup(&self, _terms: &[String]) -> Result<Vec<PublicProduct>, AppError> {
            Ok(vec![])
        }
        async fn publish(&self, _vendor: Uuid, id: Uuid) -> Result<(), AppError> {
            let mut all = self.products.lock().unwrap();
            if let Some(p) = all.iter_mut().find(|p| p.id == id) {
                p.status = "published".into();
            }
            Ok(())
        }
        async fn set_certification(
            &self,
            _vendor: Option<Uuid>,
            id: Uuid,
            to: &str,
            _note: Option<&str>,
        ) -> Result<(), AppError> {
            // The real one locks the row and checks the edge; the fake only needs the edge,
            // because that is the rule being tested.
            let mut all = self.products.lock().unwrap();
            let Some(p) = all.iter_mut().find(|p| p.id == id) else {
                return Err(AppError::not_found("product"));
            };
            certification_transition(&p.certification, to)?;
            p.certification = to.to_string();
            Ok(())
        }
    }

    fn bulb(switch: bool) -> ThingModel {
        let mut caps = vec![serde_json::json!({
            "name": "brightness",
            "points": { "level": { "dp": "2", "kind": "int" } }
        })];
        if switch {
            caps.insert(
                0,
                serde_json::json!({
                    "name": "switch",
                    "points": { "power": { "dp": "1", "kind": "bool" } }
                }),
            );
        }
        serde_json::from_value(serde_json::json!({
            "product_key": "hv_test_bulb",
            "name": "Test Bulb",
            "category": "light",
            "capabilities": caps
        }))
        .unwrap()
    }

    const V: Uuid = Uuid::nil();

    fn svc() -> CatalogService<Fake> {
        CatalogService::new(Arc::new(Fake::default()))
    }

    #[tokio::test]
    async fn a_draft_may_be_incomplete() {
        let p = svc().create(V, bulb(false), false).await.unwrap();
        assert_eq!(p.status, "draft");
    }

    #[tokio::test]
    async fn publishing_an_incomplete_model_is_refused_with_every_violation() {
        let svc = svc();
        let p = svc.create(V, bulb(false), false).await.unwrap();
        let err = svc.publish(V, p.id).await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);
        assert!(err.public_message().contains("problem"), "{err:?}");
    }

    #[tokio::test]
    async fn a_valid_model_publishes() {
        let svc = svc();
        let p = svc.create(V, bulb(true), false).await.unwrap();
        let published = svc.publish(V, p.id).await.unwrap();
        assert_eq!(published.status, "published");
    }

    #[tokio::test]
    async fn a_published_product_cannot_be_edited() {
        let svc = svc();
        let p = svc.create(V, bulb(true), true).await.unwrap();
        let err = svc.update(V, p.id, bulb(true)).await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::Conflict);
    }

    #[tokio::test]
    async fn a_product_key_cannot_be_renamed() {
        let svc = svc();
        let p = svc.create(V, bulb(true), false).await.unwrap();
        let mut renamed = bulb(true);
        renamed.product_key = "hv_other".into();
        let err = svc.update(V, p.id, renamed).await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);
    }

    #[tokio::test]
    async fn a_malformed_key_is_refused_before_it_reaches_the_database() {
        let svc = svc();
        let mut model = bulb(true);
        model.product_key = "HV Not A Key".into();
        let err = svc.create(V, model, false).await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);
    }

    #[test]
    fn certification_only_moves_along_the_edges_we_meant() {
        // The happy path, both ways out of submitted, and the revoke that has to be
        // re-submitted rather than undone.
        for (from, to) in [
            ("uncertified", "submitted"),
            ("submitted", "certified"),
            ("submitted", "uncertified"),
            ("certified", "revoked"),
            ("revoked", "submitted"),
        ] {
            assert!(
                certification_transition(from, to).is_ok(),
                "{from} -> {to} should be allowed"
            );
        }
    }

    #[test]
    fn certification_refuses_the_shortcuts() {
        // Skipping the test is the one that matters: no product goes straight from
        // uncertified to certified, because that is the whole point of the gate.
        for (from, to) in [
            ("uncertified", "certified"),
            ("revoked", "certified"),
            ("certified", "uncertified"),
            ("certified", "submitted"),
        ] {
            let err = certification_transition(from, to).unwrap_err();
            assert_eq!(err.code_kind(), ErrorCode::Conflict, "{from} -> {to}");
        }
    }

    #[test]
    fn certification_is_idempotent_and_rejects_nonsense() {
        // A retried POST is not an error the caller has to interpret.
        assert!(certification_transition("certified", "certified").is_ok());

        let err = certification_transition("uncertified", "approved").unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);
    }
}
