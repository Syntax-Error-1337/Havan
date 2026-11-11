//! Object storage for firmware images and product icons.
//!
//! One trait object, three backends: local disk in dev, MinIO in CI, S3/R2 in
//! production. The vendor portal hands out presigned URLs, so a 40 MB firmware
//! image never travels through the API process.

use bytes::Bytes;
use kernel::AppError;
use object_store::path::Path as ObjPath;
use object_store::{ObjectStore, PutPayload};
use std::sync::Arc;

#[derive(Clone)]
pub struct Objects {
    store: Arc<dyn ObjectStore>,
}

impl Objects {
    pub fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self { store }
    }

    /// Dev/test backend rooted at a local directory.
    pub fn local(dir: impl AsRef<std::path::Path>) -> Result<Self, AppError> {
        let store = object_store::local::LocalFileSystem::new_with_prefix(dir)
            .map_err(|e| AppError::Internal(format!("object store: {e}")))?;
        Ok(Self::new(Arc::new(store)))
    }

    pub async fn put(&self, key: &str, bytes: Bytes) -> Result<(), AppError> {
        self.store
            .put(&ObjPath::from(key), PutPayload::from(bytes))
            .await
            .map(|_| ())
            .map_err(|e| AppError::Internal(format!("object put {key}: {e}")))
    }

    pub async fn get(&self, key: &str) -> Result<Bytes, AppError> {
        let r = self
            .store
            .get(&ObjPath::from(key))
            .await
            .map_err(|e| AppError::Internal(format!("object get {key}: {e}")))?;
        r.bytes()
            .await
            .map_err(|e| AppError::Internal(format!("object read {key}: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn round_trips_a_blob() {
        let dir = std::env::temp_dir().join(format!("haven-obj-{}", kernel::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let objs = Objects::local(&dir).unwrap();
        objs.put("fw/a.bin", Bytes::from_static(b"hello"))
            .await
            .unwrap();
        assert_eq!(&objs.get("fw/a.bin").await.unwrap()[..], b"hello");
        std::fs::remove_dir_all(dir).ok();
    }
}
