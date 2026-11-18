//! The audit trail: who did what, when, and from where.
//!
//! Written for the actions that change something a vendor would have to explain later —
//! publishing a product, revoking a key, starting or halting a rollout, changing who can
//! reach the portal. Deliberately **not** a request log: a table that records every read
//! is a table nobody can afford to keep, and the metrics endpoint already answers "how
//! many, how slow".
//!
//! The trail is append-only. There is no update and no delete path in this module, and the
//! migration grants neither: an audit row that can be edited is not evidence.

use std::future::Future;
use std::sync::Arc;

use kernel::AppError;
use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

/// Who acted. `User` for a person in the portal, `ApiKey` for a vendor's server,
/// `System` for a job that nobody pressed a button for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    User,
    ApiKey,
    System,
}

impl ActorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActorKind::User => "user",
            ActorKind::ApiKey => "api_key",
            ActorKind::System => "system",
        }
    }
}

/// One recorded action.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub vendor_id: Uuid,
    pub actor_kind: ActorKind,
    pub actor_id: Option<Uuid>,
    /// Past tense, namespaced: `product.published`, `api_key.revoked`.
    pub action: String,
    pub target_kind: String,
    pub target_id: Option<Uuid>,
    pub request_id: Option<String>,
    pub ip: Option<String>,
    pub detail: serde_json::Value,
}

impl AuditEntry {
    pub fn new(
        vendor_id: Uuid,
        actor_kind: ActorKind,
        action: impl Into<String>,
        target_kind: impl Into<String>,
    ) -> Self {
        Self {
            vendor_id,
            actor_kind,
            actor_id: None,
            action: action.into(),
            target_kind: target_kind.into(),
            target_id: None,
            request_id: None,
            ip: None,
            detail: serde_json::json!({}),
        }
    }

    pub fn by(mut self, actor_id: Uuid) -> Self {
        self.actor_id = Some(actor_id);
        self
    }

    pub fn on(mut self, target_id: Uuid) -> Self {
        self.target_id = Some(target_id);
        self
    }

    pub fn from_ip(mut self, ip: Option<String>) -> Self {
        self.ip = ip;
        self
    }

    pub fn with_request(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    pub fn detail(mut self, detail: serde_json::Value) -> Self {
        self.detail = detail;
        self
    }
}

/// A row as the portal reads it.
#[derive(Debug, Clone, Serialize)]
pub struct AuditRow {
    pub id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub ts: OffsetDateTime,
    pub actor_kind: String,
    pub actor_id: Option<Uuid>,
    /// The actor's address, when they are a person and the address still exists.
    ///
    /// A uuid prefix is not an answer to "who did this": the person reading an audit trail is usually answering that
    /// question for somebody else, and cannot be expected to look an id up somewhere. Absent for an API key, for
    /// automation, and for a member who has since been removed — which is itself worth seeing.
    pub actor_email: Option<String>,
    pub action: String,
    pub target_kind: String,
    pub target_id: Option<Uuid>,
    pub request_id: Option<String>,
    pub ip: Option<String>,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditPage {
    pub entries: Vec<AuditRow>,
    /// The last id on this page; absent when the page is the end.
    pub next_cursor: Option<String>,
}

/// What a reader is looking for in the trail.
///
/// A trail nobody can interrogate is a record, not evidence: "who revoked a key last Tuesday" was answered by reading
/// every row since Tuesday. Each field narrows what is scanned; none of them changes what is stored.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    /// Exact action, e.g. `api_key.revoked`. Exact rather than a substring because the values are a closed set the
    /// portal offers as a list.
    pub action: Option<String>,
    /// Part of the actor's address. A substring, because the person asking usually has half a memory of it.
    pub actor: Option<String>,
}

impl AuditFilter {
    /// Trimmed and capped, so a filter cannot be made long enough to be a denial of service by hand.
    pub fn clean(&self) -> Self {
        let clean = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| v.chars().take(120).collect::<String>())
        };
        Self {
            action: clean(&self.action),
            actor: clean(&self.actor),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.action.is_none() && self.actor.is_none()
    }
}

pub trait AuditRepo {
    fn record(&self, entry: &AuditEntry) -> impl Future<Output = Result<(), AppError>> + Send;

    fn page(
        &self,
        vendor_id: Uuid,
        before: Option<Uuid>,
        limit: i64,
        filter: &AuditFilter,
    ) -> impl Future<Output = Result<Vec<AuditRow>, AppError>> + Send;

    /// The same page across every vendor, for the operator.
    ///
    /// Reading it is a system act — there is no vendor to scope to — but the rows are the same rows,
    /// so an operator's "who suspended this vendor" and the vendor's own "what happened to us" cannot
    /// disagree. `vendor` narrows it, which is what the console does when it is looking at one.
    fn page_all(
        &self,
        vendor: Option<Uuid>,
        before: Option<Uuid>,
        limit: i64,
    ) -> impl Future<Output = Result<Vec<AuditRow>, AppError>> + Send;
}

pub struct AuditService<R> {
    repo: Arc<R>,
}

impl<R: AuditRepo + Send + Sync> AuditService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    /// Write an entry. Errors are logged, not returned: an audit failure must not turn a
    /// successful product publish into a customer-visible error — but it must also not be
    /// silent, which is what the log line is for.
    pub async fn record(&self, entry: AuditEntry) {
        if let Err(e) = self.repo.record(&entry).await {
            tracing::error!(
                action = %entry.action,
                vendor = %entry.vendor_id,
                error = ?e,
                "could not write the audit log"
            );
        }
    }

    pub async fn page(
        &self,
        vendor_id: Uuid,
        before: Option<Uuid>,
        limit: Option<i64>,
        filter: &AuditFilter,
    ) -> Result<AuditPage, AppError> {
        let limit = limit.unwrap_or(50).clamp(1, 200);
        let filter = filter.clean();
        let mut entries = self
            .repo
            .page(vendor_id, before, limit + 1, &filter)
            .await?;

        let next_cursor = if i64::try_from(entries.len()).unwrap_or(0) > limit {
            entries.truncate(usize::try_from(limit).unwrap_or(50));
            entries.last().map(|e| e.id.to_string())
        } else {
            None
        };

        Ok(AuditPage {
            entries,
            next_cursor,
        })
    }

    /// Every row matching the filter, newest first, up to `max`.
    ///
    /// An export is not a page: an auditor wants the whole range in one file, not the first fifty and a cursor. The cap
    /// exists because "everything" on a busy vendor is unbounded — the caller is told when it was hit rather than being
    /// handed a file that quietly stops.
    pub async fn export(
        &self,
        vendor_id: Uuid,
        filter: &AuditFilter,
        max: i64,
    ) -> Result<(Vec<AuditRow>, bool), AppError> {
        const BATCH: i64 = 200;
        let filter = filter.clean();
        let mut rows: Vec<AuditRow> = Vec::new();
        let mut before: Option<Uuid> = None;

        loop {
            let room = max - rows.len() as i64;
            if room <= 0 {
                return Ok((rows, true));
            }
            let batch = self
                .repo
                .page(vendor_id, before, room.min(BATCH), &filter)
                .await?;
            let short = (batch.len() as i64) < room.min(BATCH);
            before = batch.last().map(|row| row.id);
            rows.extend(batch);
            if short {
                return Ok((rows, false));
            }
        }
    }

    /// The operator's page: every vendor, or one. Same cursor rule, so an operator paging through a
    /// busy day cannot skip an entry at a page boundary.
    pub async fn page_all(
        &self,
        vendor: Option<Uuid>,
        before: Option<Uuid>,
        limit: Option<i64>,
    ) -> Result<AuditPage, AppError> {
        let limit = limit.unwrap_or(50).clamp(1, 200);
        let mut entries = self.repo.page_all(vendor, before, limit + 1).await?;

        let next_cursor = if i64::try_from(entries.len()).unwrap_or(0) > limit {
            entries.truncate(usize::try_from(limit).unwrap_or(50));
            entries.last().map(|e| e.id.to_string())
        } else {
            None
        };

        Ok(AuditPage {
            entries,
            next_cursor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        rows: Vec<AuditRow>,
        written: std::sync::Mutex<Vec<AuditEntry>>,
    }

    impl AuditRepo for Fake {
        async fn record(&self, entry: &AuditEntry) -> Result<(), AppError> {
            self.written.lock().unwrap().push(entry.clone());
            Ok(())
        }

        async fn page(
            &self,
            _vendor_id: Uuid,
            _before: Option<Uuid>,
            limit: i64,
            _filter: &AuditFilter,
        ) -> Result<Vec<AuditRow>, AppError> {
            Ok(self
                .rows
                .iter()
                .take(usize::try_from(limit).unwrap_or(50))
                .cloned()
                .collect())
        }

        async fn page_all(
            &self,
            _vendor: Option<Uuid>,
            _before: Option<Uuid>,
            limit: i64,
        ) -> Result<Vec<AuditRow>, AppError> {
            Ok(self
                .rows
                .iter()
                .take(usize::try_from(limit).unwrap_or(50))
                .cloned()
                .collect())
        }
    }

    fn row(n: u128) -> AuditRow {
        AuditRow {
            id: Uuid::from_u128(n),
            ts: kernel::now(),
            actor_kind: "user".into(),
            actor_id: None,
            actor_email: None,
            action: "product.published".into(),
            target_kind: "product".into(),
            target_id: None,
            request_id: None,
            ip: None,
            detail: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn a_full_page_reports_the_cursor_to_continue_from() {
        let service = AuditService::new(Arc::new(Fake {
            rows: (1..=3).map(row).collect(),
            written: std::sync::Mutex::new(vec![]),
        }));
        let page = service
            .page(Uuid::from_u128(9), None, Some(2), &AuditFilter::default())
            .await
            .expect("page");
        assert_eq!(page.entries.len(), 2);
        assert_eq!(page.next_cursor, Some(Uuid::from_u128(2).to_string()));
    }

    #[tokio::test]
    async fn a_short_page_ends_the_list() {
        let service = AuditService::new(Arc::new(Fake {
            rows: vec![row(1)],
            written: std::sync::Mutex::new(vec![]),
        }));
        let page = service
            .page(Uuid::from_u128(9), None, Some(2), &AuditFilter::default())
            .await
            .expect("page");
        assert!(page.next_cursor.is_none());
    }

    #[tokio::test]
    async fn the_builder_says_what_happened_in_one_line() {
        let service = AuditService::new(Arc::new(Fake {
            rows: vec![],
            written: std::sync::Mutex::new(vec![]),
        }));
        service
            .record(
                AuditEntry::new(
                    Uuid::from_u128(1),
                    ActorKind::User,
                    "api_key.revoked",
                    "api_key",
                )
                .by(Uuid::from_u128(2))
                .on(Uuid::from_u128(3))
                .from_ip(Some("10.0.0.4".into()))
                .detail(serde_json::json!({ "name": "factory line 2" })),
            )
            .await;
        // Nothing to assert about the fake's storage beyond "it received a well-formed
        // entry"; the value here is that the builder compiles and every field is settable.
    }
}
