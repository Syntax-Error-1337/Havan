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

    fn publish(
        &self,
        vendor_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Published products by advertisement hash or key. Cross-tenant by design:
    /// a published product is public, and a radar does not know which vendor it
    /// is looking at.
    fn public_lookup(
        &self,
        terms: &[String],
    ) -> impl Future<Output = Result<Vec<PublicProduct>, AppError>> + Send;
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
            schema_version: i32::from(model.schema_version),
            thing_model: model,
            created_at: kernel::now(),
        };

        self.repo.insert(vendor_id, &product).await?;
        Ok(product)
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
}
