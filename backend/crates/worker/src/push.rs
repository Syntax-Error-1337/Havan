//! Push delivery: the part that reaches a phone in a pocket.
//!
//! ## What this replaces
//!
//! Alerts were written to `notifications` and read over the websocket. With the app closed
//! — which is most of the time — nothing arrived, so "your hallway motion sensor tripped"
//! was a row nobody saw.
//!
//! ## Shape
//!
//! `Sender` walks undelivered notifications, resolves the user's device tokens, and hands
//! each one to a [`PushTransport`]. The transport is a trait so the platform is not welded to
//! Apple: the log transport runs in development and in tests, `Apns` is the real one, and an
//! FCM implementation would slot in beside it without touching the queue.
//!
//! ## What is verified and what is not
//!
//! The queue, the dedupe, the retry budget and the request an APNs client would send are
//! covered by tests. **No live APNs call has been made** — that needs an Apple developer
//! account, a `.p8` key and a real device token, none of which exist here. The transport is
//! opt-in (`HAVEN_APNS_KEY_FILE`), so a deployment without credentials uses the log
//! transport and says so at startup rather than failing silently at 3 a.m.

use std::time::Duration;

use domain::notification::{Delivery, PushToken};
use kernel::AppError;

/// Walks the queue and hands each notification to the transport.
///
/// One notification per user per token: a person with a phone and a tablet gets two pushes,
/// which is what they asked for by installing the app twice.
/// Generic over the transport rather than `dyn`: an `impl Future` in the trait is what keeps
/// it readable, and that is not object-safe. The worker picks one transport at startup, so
/// there is nothing to swap at runtime.
pub struct Sender<P: domain::notification::PushRepo, T: PushTransport> {
    repo: std::sync::Arc<P>,
    transport: std::sync::Arc<T>,
}

/// What one pass over the queue did, for the log line a sweep should leave behind.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Sweep {
    pub delivered: usize,
    pub no_tokens: usize,
    pub failed: usize,
    pub pruned_tokens: usize,
}

impl<P: domain::notification::PushRepo, T: PushTransport> Sender<P, T> {
    pub fn new(repo: std::sync::Arc<P>, transport: std::sync::Arc<T>) -> Self {
        Self { repo, transport }
    }

    pub async fn tick(&self, limit: i64) -> Result<Sweep, AppError> {
        let mut sweep = Sweep::default();

        for message in self.repo.undelivered(limit).await? {
            let tokens = self.repo.tokens_for(message.user_id).await?;

            // Nobody to tell. Done, not pending: retrying forever would grow the queue with
            // rows that can never succeed.
            if tokens.is_empty() {
                self.repo.mark_delivered(message.id).await?;
                sweep.no_tokens += 1;
                continue;
            }

            let mut any_sent = false;
            let mut last_error: Option<String> = None;

            for token in &tokens {
                match self.transport.send(token, &message).await {
                    Ok(Outcome::Sent) => any_sent = true,
                    Ok(Outcome::Unregistered) => {
                        self.repo.prune_token(token.id).await?;
                        sweep.pruned_tokens += 1;
                    }
                    Err(e) => {
                        last_error = Some(e.to_string());
                    }
                }
            }

            if any_sent {
                self.repo.mark_delivered(message.id).await?;
                sweep.delivered += 1;
            } else if let Some(error) = last_error {
                self.repo.mark_attempt(message.id, &error).await?;
                sweep.failed += 1;
            } else {
                // Every token was unregistered: nothing left to try, so it is settled.
                self.repo.mark_delivered(message.id).await?;
                sweep.delivered += 1;
            }
        }

        Ok(sweep)
    }
}

/// What happened to one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Handed to the provider.
    Sent,
    /// The token is dead — the provider said the device is gone. Prune it.
    Unregistered,
}

/// A push provider.
pub trait PushTransport: Send + Sync {
    fn name(&self) -> &'static str;
    fn send(
        &self,
        token: &PushToken,
        message: &Delivery,
    ) -> impl Future<Output = Result<Outcome, AppError>> + Send;
}

/// Development transport: records that it *would* have sent, and says so in the log.
///
/// Deliberately not a no-op that returns success silently — the log line is how someone
/// running locally knows which notifications the real thing would have pushed.
pub struct LogTransport;

impl PushTransport for LogTransport {
    fn name(&self) -> &'static str {
        "log"
    }

    async fn send(&self, token: &PushToken, message: &Delivery) -> Result<Outcome, AppError> {
        tracing::info!(
            platform = %token.platform,
            user = %token.user_id,
            title = %message.title,
            body = %message.body,
            "push (log transport): would have delivered"
        );
        Ok(Outcome::Sent)
    }
}

/// Apple Push Notification service, token-based (`.p8`) authentication.
///
/// Written against the documented contract: an ES256 JWT whose `iss` is the team id, `kid`
/// the key id, `iat` the issue time; `POST /3/device/{token}` with `apns-topic` set to the
/// bundle id and `apns-push-type: alert`.
///
/// **Unverified against Apple.** The token construction is unit-tested because it is the part
/// that can be tested; the delivery itself needs credentials.
pub struct Apns {
    endpoint: String,
    topic: String,
    jwt: String,
    http: reqwest::Client,
}

impl Apns {
    /// `endpoint` is `https://api.push.apple.com` in production and
    /// `https://api.sandbox.push.apple.com` for development builds.
    pub fn new(
        endpoint: impl Into<String>,
        topic: impl Into<String>,
        key_id: &str,
        team_id: &str,
        key_pem: &str,
    ) -> Result<Self, AppError> {
        let jwt = apns_jwt(key_id, team_id, kernel::now_s(), key_pem)?;
        let http = reqwest::Client::builder()
            // APNs requires HTTP/2 and rejects anything older.
            .http2_prior_knowledge()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| AppError::Internal(format!("push: could not build client: {e}")))?;

        Ok(Self {
            endpoint: endpoint.into(),
            topic: topic.into(),
            jwt,
            http,
        })
    }

    /// The request APNs expects, built separately so it can be asserted without a network.
    pub fn request(&self, token: &PushToken, message: &Delivery) -> ApnsRequest {
        ApnsRequest {
            url: format!("{}/3/device/{}", self.endpoint, token.token),
            authorization: format!("bearer {}", self.jwt),
            topic: self.topic.clone(),
            payload: serde_json::json!({
                "aps": {
                    "alert": { "title": message.title, "body": message.body },
                    "sound": "default",
                    // The app deep-links from the data, so a tap opens the right screen.
                    "mutable-content": 1,
                },
                "data": message.data,
            }),
        }
    }
}

/// The parts of an APNs request that matter, kept as data so a test can look at them.
#[derive(Debug, Clone)]
pub struct ApnsRequest {
    pub url: String,
    pub authorization: String,
    pub topic: String,
    pub payload: serde_json::Value,
}

impl PushTransport for Apns {
    fn name(&self) -> &'static str {
        "apns"
    }

    async fn send(&self, token: &PushToken, message: &Delivery) -> Result<Outcome, AppError> {
        let request = self.request(token, message);
        let response = self
            .http
            .post(&request.url)
            .header("authorization", &request.authorization)
            .header("apns-topic", &request.topic)
            .header("apns-push-type", "alert")
            .json(&request.payload)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("push: apns request failed: {e}")))?;

        match response.status().as_u16() {
            200 => Ok(Outcome::Sent),
            // 410 is Apple's "this token is no longer valid", the one answer the sender must
            // act on by deleting the token.
            410 => Ok(Outcome::Unregistered),
            other => {
                let body = response.text().await.unwrap_or_default();
                Err(AppError::Internal(format!(
                    "push: apns answered {other}: {}",
                    body.chars().take(200).collect::<String>()
                )))
            }
        }
    }
}

/// The provider token: ES256 over `{iss, iat}`, signed with the `.p8` key.
fn apns_jwt(
    key_id: &str,
    team_id: &str,
    issued_at: i64,
    key_pem: &str,
) -> Result<String, AppError> {
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
    header.kid = Some(key_id.to_string());

    let claims = serde_json::json!({ "iss": team_id, "iat": issued_at });

    let key = jsonwebtoken::EncodingKey::from_ec_pem(key_pem.as_bytes())
        .map_err(|e| AppError::Internal(format!("push: bad APNs key: {e}")))?;

    jsonwebtoken::encode(&header, &claims, &key)
        .map_err(|e| AppError::Internal(format!("push: could not sign APNs token: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(platform: &str, value: &str) -> PushToken {
        PushToken {
            id: uuid::Uuid::now_v7(),
            user_id: uuid::Uuid::now_v7(),
            platform: platform.into(),
            token: value.into(),
            app_version: None,
        }
    }

    fn message() -> Delivery {
        Delivery {
            id: uuid::Uuid::now_v7(),
            user_id: uuid::Uuid::now_v7(),
            kind: "security".into(),
            title: "Front door opened".into(),
            body: "Nobody was home.".into(),
            data: serde_json::json!({ "device_id": "d1" }),
        }
    }

    #[tokio::test]
    async fn the_log_transport_never_reports_a_dead_token() {
        // It cannot know, and inventing "unregistered" would prune real devices.
        let outcome = LogTransport.send(&token("ios", "abc"), &message()).await;
        assert_eq!(outcome.expect("send"), Outcome::Sent);
    }

    #[test]
    fn an_apns_request_carries_the_alert_and_the_data() {
        // A throwaway EC key (PKCS#8 — `from_ec_pem` rejects the SEC1 form), so the JWT is
        // signed the way production signs it.
        let apns = Apns::new(
            "https://api.sandbox.push.apple.com",
            "com.haven.app",
            "KEYID12345",
            "TEAMID1234",
            EC_KEY,
        )
        .expect("apns");

        let request = apns.request(&token("ios", "device-token-1"), &message());
        assert_eq!(
            request.url,
            "https://api.sandbox.push.apple.com/3/device/device-token-1"
        );
        assert_eq!(request.topic, "com.haven.app");
        assert!(request.authorization.starts_with("bearer "));
        assert_eq!(
            request.payload["aps"]["alert"]["title"],
            serde_json::json!("Front door opened")
        );
        // The deep-link data has to ride along, or tapping the alert opens the home screen.
        assert_eq!(
            request.payload["data"]["device_id"],
            serde_json::json!("d1")
        );
    }

    #[test]
    fn a_bad_key_fails_at_startup_not_at_three_am() {
        let err = Apns::new("https://example.test", "topic", "K", "T", "not a key");
        assert!(err.is_err(), "a malformed .p8 must not build a transport");
    }

    use std::sync::Mutex;

    /// A repo whose behaviour each test can set: this is where "no tokens", "provider says
    /// the token is dead" and "provider failed" are distinguished, which is the whole point
    /// of the sender existing.
    #[derive(Default)]
    struct FakeRepo {
        queue: Mutex<Vec<Delivery>>,
        tokens: Mutex<Vec<PushToken>>,
        delivered: Mutex<Vec<uuid::Uuid>>,
        attempted: Mutex<Vec<(uuid::Uuid, String)>>,
        pruned: Mutex<Vec<uuid::Uuid>>,
    }

    impl domain::notification::PushRepo for FakeRepo {
        async fn undelivered(&self, _limit: i64) -> Result<Vec<Delivery>, AppError> {
            Ok(self.queue.lock().unwrap().clone())
        }

        async fn tokens_for(&self, _user_id: uuid::Uuid) -> Result<Vec<PushToken>, AppError> {
            Ok(self.tokens.lock().unwrap().clone())
        }

        async fn mark_delivered(&self, id: uuid::Uuid) -> Result<(), AppError> {
            self.delivered.lock().unwrap().push(id);
            Ok(())
        }

        async fn mark_attempt(&self, id: uuid::Uuid, error: &str) -> Result<(), AppError> {
            self.attempted.lock().unwrap().push((id, error.to_string()));
            Ok(())
        }

        async fn prune_token(&self, id: uuid::Uuid) -> Result<(), AppError> {
            self.pruned.lock().unwrap().push(id);
            Ok(())
        }

        async fn register_token(
            &self,
            _user_id: uuid::Uuid,
            _platform: &str,
            _token: &str,
            _app_version: Option<&str>,
        ) -> Result<(), AppError> {
            Ok(())
        }

        async fn remove_token(&self, _user_id: uuid::Uuid, _token: &str) -> Result<bool, AppError> {
            Ok(false)
        }
    }

    /// Always fails, so the retry path can be exercised.
    struct FailingTransport;

    impl PushTransport for FailingTransport {
        fn name(&self) -> &'static str {
            "failing"
        }

        async fn send(&self, _t: &PushToken, _m: &Delivery) -> Result<Outcome, AppError> {
            Err(AppError::Internal("provider is down".into()))
        }
    }

    /// Always reports the token dead, so pruning can be exercised.
    struct GoneTransport;

    impl PushTransport for GoneTransport {
        fn name(&self) -> &'static str {
            "gone"
        }

        async fn send(&self, _t: &PushToken, _m: &Delivery) -> Result<Outcome, AppError> {
            Ok(Outcome::Unregistered)
        }
    }

    #[tokio::test]
    async fn a_notification_with_nobody_to_tell_is_done_not_pending() {
        // The queue would otherwise grow forever with rows that can never succeed.
        let repo = std::sync::Arc::new(FakeRepo::default());
        repo.queue.lock().unwrap().push(message());

        let sender = Sender::new(repo.clone(), std::sync::Arc::new(LogTransport));
        let sweep = sender.tick(10).await.expect("tick");

        assert_eq!(sweep.no_tokens, 1);
        assert_eq!(repo.delivered.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_failure_is_retried_rather_than_dropped() {
        let repo = std::sync::Arc::new(FakeRepo::default());
        repo.queue.lock().unwrap().push(message());
        repo.tokens.lock().unwrap().push(token("ios", "abc"));

        let sender = Sender::new(repo.clone(), std::sync::Arc::new(FailingTransport));
        let sweep = sender.tick(10).await.expect("tick");

        assert_eq!(sweep.failed, 1);
        assert!(repo.delivered.lock().unwrap().is_empty(), "not settled");
        assert_eq!(repo.attempted.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_dead_token_is_pruned_and_the_message_settled() {
        let repo = std::sync::Arc::new(FakeRepo::default());
        repo.queue.lock().unwrap().push(message());
        repo.tokens.lock().unwrap().push(token("ios", "dead"));

        let sender = Sender::new(repo.clone(), std::sync::Arc::new(GoneTransport));
        let sweep = sender.tick(10).await.expect("tick");

        assert_eq!(sweep.pruned_tokens, 1);
        assert_eq!(
            repo.delivered.lock().unwrap().len(),
            1,
            "nothing left to try"
        );
    }

    /// A throwaway P-256 key in PKCS#8 form, generated for this test file only
    /// (`openssl ecparam -genkey` then `openssl pkcs8 -topk8`). It signs nothing real and is
    /// committed on purpose: the test asserts the request APNs would receive, not that Apple
    /// accepts it.
    const EC_KEY: &str = "-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgmbDDUjsQqI3jamJE
QnYAhZEn6xoVUrG7N1gqABrArFuhRANCAAQ6KDfZd1IYq0OK0w5obQ9rdXm56t5w
isVl3bTXwDBu9aabnJ7pbhTIM5QkWXwpXcvn3eiWjok2lMerQeN5v40e
-----END PRIVATE KEY-----";
}
