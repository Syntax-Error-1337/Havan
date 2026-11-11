//! Who a person is, and how a session is proven.
//!
//! Access tokens are short-lived and stateless (HS256 JWT). Refresh tokens are
//! opaque, stored hashed, rotated on every use, and grouped into a family so
//! that replaying a stolen one revokes the whole lineage instead of quietly
//! issuing a second session to the thief.

use std::sync::Arc;
use std::time::Duration;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use base64::Engine;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use kernel::{AppError, ErrorCode};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

pub const ACCESS_TTL: Duration = Duration::from_secs(15 * 60);
pub const REFRESH_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);
pub const MIN_PASSWORD_LEN: usize = 10;

#[derive(Debug, Clone)]
pub struct User {
    pub id: Uuid,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub display_name: String,
    pub password_hash: Option<String>,
    pub locale: String,
}

#[derive(Debug, Clone)]
pub struct RefreshRow {
    pub id: Uuid,
    /// Exactly one of these is set; a session belongs to one kind of principal.
    pub user_id: Option<Uuid>,
    pub vendor_user_id: Option<Uuid>,
    /// `Some` for a vendor session. Defaults to `None` for a customer session.
    pub vendor_id: Option<Uuid>,
    pub family_id: Uuid,
    pub hash: String,
    pub expires_at: OffsetDateTime,
    pub used_at: Option<OffsetDateTime>,
    pub revoked_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    /// Seconds until the access token expires.
    pub expires_in: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    /// Session (refresh family) id, so a single login can be revoked.
    pub sid: Uuid,
    /// Present only on a vendor-session token: the tenant this request acts for.
    /// A customer token never carries it, and RLS is the thing that enforces
    /// what that means.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vid: Option<Uuid>,
    pub exp: i64,
    pub iat: i64,
}

/// A portal user. Separate table from customers: different lifecycle, different
/// tenancy, and no reason to share a password policy change with them.
#[derive(Debug, Clone)]
pub struct VendorUser {
    pub id: Uuid,
    pub vendor_id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub role: String,
}

#[derive(Debug, Clone)]
pub struct RegisterInput {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

/// What the slice needs from storage. Kept small on purpose: every method is a
/// thing a fake must implement for the service tests.
pub trait IdentityRepo {
    fn find_by_email(
        &self,
        email: &str,
    ) -> impl Future<Output = Result<Option<User>, AppError>> + Send;

    fn find_by_id(&self, id: Uuid) -> impl Future<Output = Result<Option<User>, AppError>> + Send;

    fn insert_user(&self, user: &User) -> impl Future<Output = Result<(), AppError>> + Send;

    fn insert_refresh(&self, row: &RefreshRow)
    -> impl Future<Output = Result<(), AppError>> + Send;

    fn find_refresh(
        &self,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<RefreshRow>, AppError>> + Send;

    fn mark_used(&self, id: Uuid) -> impl Future<Output = Result<(), AppError>> + Send;

    fn revoke_family(&self, family_id: Uuid) -> impl Future<Output = Result<(), AppError>> + Send;

    fn find_vendor_user(
        &self,
        vendor_id: Uuid,
        email: &str,
    ) -> impl Future<Output = Result<Option<VendorUser>, AppError>> + Send;
}

pub struct IdentityService<R> {
    repo: Arc<R>,
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl<R: IdentityRepo + Send + Sync> IdentityService<R> {
    pub fn new(repo: Arc<R>, secret: &str) -> Self {
        Self {
            repo,
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
        }
    }

    pub async fn register(&self, input: RegisterInput) -> Result<Tokens, AppError> {
        let email = normalize_email(&input.email);
        validate_email(&email)?;
        validate_password(&input.password)?;

        if self.repo.find_by_email(&email).await?.is_some() {
            return Err(AppError::code(
                ErrorCode::Conflict,
                "an account with that email already exists",
            ));
        }

        let user = User {
            id: kernel::new_id(),
            email: Some(email),
            phone: None,
            display_name: if input.display_name.trim().is_empty() {
                "Friend".to_string()
            } else {
                input.display_name.trim().to_string()
            },
            password_hash: Some(hash_password(&input.password)?),
            locale: "en".to_string(),
        };
        self.repo.insert_user(&user).await?;
        self.issue_session(user.id).await
    }

    pub async fn login(&self, email: &str, password: &str) -> Result<Tokens, AppError> {
        let email = normalize_email(email);
        let user = self.repo.find_by_email(&email).await?;

        // Hash a dummy when the user is unknown, so response time does not
        // reveal whether an account exists.
        let Some(user) = user else {
            let _ = verify_password(password, dummy_hash());
            return Err(invalid_credentials());
        };

        let Some(hash) = user.password_hash.as_deref() else {
            return Err(invalid_credentials());
        };
        if !verify_password(password, hash) {
            return Err(invalid_credentials());
        }

        self.issue_session(user.id).await
    }

    /// Exchange a refresh token. A token that was already used means the
    /// lineage leaked: revoke every token in the family and fail.
    pub async fn refresh(&self, token: &str) -> Result<Tokens, AppError> {
        let (id, secret) = split_refresh_token(token)?;
        let row = self
            .repo
            .find_refresh(id)
            .await?
            .ok_or_else(|| AppError::code(ErrorCode::Unauthorized, "unknown refresh token"))?;

        if row.revoked_at.is_some() || row.used_at.is_some() {
            self.repo.revoke_family(row.family_id).await?;
            return Err(AppError::code(
                ErrorCode::Unauthorized,
                "this session has been revoked, please sign in again",
            ));
        }
        if row.expires_at < kernel::now() {
            return Err(AppError::code(ErrorCode::TokenExpired, "session expired"));
        }
        if hash_token(&secret) != row.hash {
            self.repo.revoke_family(row.family_id).await?;
            return Err(invalid_credentials());
        }

        self.repo.mark_used(row.id).await?;
        // The subject and the vendor ride on the stored row, not on the new
        // request: a customer token can never be upgraded by refreshing it.
        self.issue_refresh(
            row.user_id,
            row.vendor_user_id,
            row.family_id,
            row.vendor_id,
        )
        .await
    }

    /// Portal login. The customer's `login` is a different door with a
    /// different table, on purpose.
    pub async fn login_vendor(
        &self,
        vendor_id: Uuid,
        email: &str,
        password: &str,
    ) -> Result<Tokens, AppError> {
        let email = normalize_email(email);
        let Some(user) = self.repo.find_vendor_user(vendor_id, &email).await? else {
            let _ = verify_password(password, dummy_hash());
            return Err(invalid_credentials());
        };
        if !verify_password(password, &user.password_hash) {
            return Err(invalid_credentials());
        }
        self.issue_refresh(None, Some(user.id), kernel::new_id(), Some(user.vendor_id))
            .await
    }

    pub async fn logout(&self, refresh_token: &str) -> Result<(), AppError> {
        if let Ok((id, _)) = split_refresh_token(refresh_token)
            && let Some(row) = self.repo.find_refresh(id).await?
        {
            self.repo.revoke_family(row.family_id).await?;
        }
        Ok(())
    }

    /// Verify an access token and return its subject. Called by the auth
    /// middleware for every request.
    pub fn verify_access(&self, token: &str) -> Result<Claims, AppError> {
        let validation = Validation::new(Algorithm::HS256);
        decode::<Claims>(token, &self.decoding, &validation)
            .map(|d| d.claims)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                    AppError::code(ErrorCode::TokenExpired, "access token expired")
                }
                _ => AppError::code(ErrorCode::Unauthorized, "invalid access token"),
            })
    }

    pub async fn user(&self, id: Uuid) -> Result<User, AppError> {
        self.repo
            .find_by_id(id)
            .await?
            .ok_or_else(|| AppError::not_found("user"))
    }

    async fn issue_session(&self, user_id: Uuid) -> Result<Tokens, AppError> {
        self.issue_refresh(Some(user_id), None, kernel::new_id(), None)
            .await
    }

    async fn issue_refresh(
        &self,
        user_id: Option<Uuid>,
        vendor_user_id: Option<Uuid>,
        family_id: Uuid,
        vendor_id: Option<Uuid>,
    ) -> Result<Tokens, AppError> {
        // The access token's subject is whichever principal this session is for.
        let subject = user_id
            .or(vendor_user_id)
            .ok_or_else(|| AppError::Internal("a session needs a subject".into()))?;
        let id = kernel::new_id();
        let secret = random_token();
        let now = kernel::now();

        self.repo
            .insert_refresh(&RefreshRow {
                id,
                user_id,
                vendor_user_id,
                vendor_id,
                family_id,
                hash: hash_token(&secret),
                expires_at: now + time::Duration::seconds(REFRESH_TTL.as_secs() as i64),
                used_at: None,
                revoked_at: None,
            })
            .await?;

        Ok(Tokens {
            access: self.access_token(subject, family_id, now, vendor_id)?,
            refresh: format!("{id}.{secret}"),
            expires_in: ACCESS_TTL.as_secs() as i64,
        })
    }

    fn access_token(
        &self,
        user_id: Uuid,
        family_id: Uuid,
        now: OffsetDateTime,
        vendor_id: Option<Uuid>,
    ) -> Result<String, AppError> {
        let claims = Claims {
            sub: user_id,
            sid: family_id,
            vid: vendor_id,
            exp: (now + time::Duration::seconds(ACCESS_TTL.as_secs() as i64)).unix_timestamp(),
            iat: now.unix_timestamp(),
        };
        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .map_err(|e| AppError::Internal(format!("token encoding: {e}")))
    }
}

fn invalid_credentials() -> AppError {
    AppError::code(
        ErrorCode::InvalidCredentials,
        "email or password is incorrect",
    )
}

pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

fn validate_email(email: &str) -> Result<(), AppError> {
    // Deliberately permissive: deliverability is proven by the verification
    // mail, not by a regex that rejects valid addresses.
    let ok = email.len() >= 3
        && email.len() <= 254
        && email.contains('@')
        && !email.starts_with('@')
        && !email.ends_with('@')
        && !email.contains(' ');
    if ok {
        Ok(())
    } else {
        Err(AppError::validation(
            "that email address does not look right",
        ))
    }
}

fn validate_password(password: &str) -> Result<(), AppError> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::validation(format!(
            "password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    if password.len() > 256 {
        return Err(AppError::validation("password is too long"));
    }
    Ok(())
}

pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut rand::rngs::OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("password hashing: {e}")))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

fn dummy_hash() -> &'static str {
    // A real Argon2id hash of a random string; verifying against it costs the
    // same as verifying a real password.
    "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHR2YWx1ZQ$8QnQ3Xk9m0KZm0h0m0Q0YQ0bQ0bQ0bQ0bQ0bQ0bQ0bQ"
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_token(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn split_refresh_token(token: &str) -> Result<(Uuid, String), AppError> {
    let (id, secret) = token
        .split_once('.')
        .ok_or_else(|| AppError::code(ErrorCode::Unauthorized, "malformed refresh token"))?;
    let id = Uuid::parse_str(id)
        .map_err(|_| AppError::code(ErrorCode::Unauthorized, "malformed refresh token"))?;
    Ok((id, secret.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeRepo {
        users: Mutex<Vec<User>>,
        refresh: Mutex<Vec<RefreshRow>>,
        revocations: Mutex<Vec<Uuid>>,
    }

    impl IdentityRepo for FakeRepo {
        async fn find_by_email(&self, email: &str) -> Result<Option<User>, AppError> {
            Ok(self
                .users
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.email.as_deref() == Some(email))
                .cloned())
        }
        async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, AppError> {
            Ok(self
                .users
                .lock()
                .unwrap()
                .iter()
                .find(|u| u.id == id)
                .cloned())
        }
        async fn insert_user(&self, user: &User) -> Result<(), AppError> {
            self.users.lock().unwrap().push(user.clone());
            Ok(())
        }
        async fn insert_refresh(&self, row: &RefreshRow) -> Result<(), AppError> {
            self.refresh.lock().unwrap().push(row.clone());
            Ok(())
        }
        async fn find_refresh(&self, id: Uuid) -> Result<Option<RefreshRow>, AppError> {
            Ok(self
                .refresh
                .lock()
                .unwrap()
                .iter()
                .find(|r| r.id == id)
                .cloned())
        }
        async fn mark_used(&self, id: Uuid) -> Result<(), AppError> {
            if let Some(r) = self.refresh.lock().unwrap().iter_mut().find(|r| r.id == id) {
                r.used_at = Some(kernel::now());
            }
            Ok(())
        }
        async fn find_vendor_user(
            &self,
            _vendor_id: Uuid,
            _email: &str,
        ) -> Result<Option<VendorUser>, AppError> {
            Ok(None)
        }
        async fn revoke_family(&self, family_id: Uuid) -> Result<(), AppError> {
            // The real repository marks every row in the family; the fake must
            // do the same or it hides a security bug.
            let now = kernel::now();
            for row in self.refresh.lock().unwrap().iter_mut() {
                if row.family_id == family_id {
                    row.revoked_at = Some(now);
                }
            }
            self.revocations.lock().unwrap().push(family_id);
            Ok(())
        }
    }

    fn service() -> IdentityService<FakeRepo> {
        IdentityService::new(
            Arc::new(FakeRepo::default()),
            "test-secret-at-least-32-bytes-long!!",
        )
    }

    fn input(email: &str, password: &str) -> RegisterInput {
        RegisterInput {
            email: email.into(),
            password: password.into(),
            display_name: "Test".into(),
        }
    }

    #[tokio::test]
    async fn register_then_login_returns_a_working_access_token() {
        let svc = service();
        let tokens = svc
            .register(input("A@Example.com ", "correct horse battery"))
            .await
            .unwrap();

        let claims = svc.verify_access(&tokens.access).unwrap();
        let user = svc.user(claims.sub).await.unwrap();
        assert_eq!(
            user.email.as_deref(),
            Some("a@example.com"),
            "email is normalised"
        );

        let again = svc
            .login("a@example.com", "correct horse battery")
            .await
            .unwrap();
        assert_ne!(
            tokens.refresh, again.refresh,
            "each login is its own session"
        );
    }

    #[tokio::test]
    async fn a_short_password_is_refused() {
        let err = service()
            .register(input("a@b.co", "short"))
            .await
            .unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::ValidationFailed);
    }

    #[tokio::test]
    async fn a_duplicate_email_is_a_conflict() {
        let svc = service();
        svc.register(input("a@b.co", "long enough pass"))
            .await
            .unwrap();
        let err = svc
            .register(input("A@B.CO", "long enough pass"))
            .await
            .unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::Conflict);
    }

    #[tokio::test]
    async fn a_wrong_password_says_nothing_useful() {
        let svc = service();
        svc.register(input("a@b.co", "long enough pass"))
            .await
            .unwrap();
        let err = svc.login("a@b.co", "wrong password").await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::InvalidCredentials);
        assert_eq!(err.public_message(), "email or password is incorrect");

        let unknown = svc.login("nobody@b.co", "whatever").await.unwrap_err();
        assert_eq!(
            unknown.public_message(),
            err.public_message(),
            "an unknown account must be indistinguishable from a wrong password"
        );
    }

    #[tokio::test]
    async fn refresh_rotates_and_a_replayed_token_kills_the_family() {
        let svc = service();
        let tokens = svc
            .register(input("a@b.co", "long enough pass"))
            .await
            .unwrap();

        let rotated = svc.refresh(&tokens.refresh).await.unwrap();
        assert_ne!(rotated.refresh, tokens.refresh);

        // The client that lost the race replays the old token.
        let replay = svc.refresh(&tokens.refresh).await;
        assert!(replay.is_err(), "a used token must not work twice");

        // ...and the whole family is now dead, including the rotated one.
        assert!(svc.refresh(&rotated.refresh).await.is_err());
    }

    #[tokio::test]
    async fn an_expired_access_token_is_reported_as_expired() {
        let svc = service();
        let claims = Claims {
            sub: Uuid::nil(),
            sid: Uuid::nil(),
            vid: None,
            exp: 1,
            iat: 1,
        };
        let token = encode(&Header::new(Algorithm::HS256), &claims, &svc.encoding).unwrap();
        let err = svc.verify_access(&token).unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::TokenExpired);
    }

    #[tokio::test]
    async fn a_token_signed_with_another_key_is_rejected() {
        let svc = service();
        let other = IdentityService::new(
            Arc::new(FakeRepo::default()),
            "another-secret-another-secret-",
        );
        let tokens = other
            .register(input("a@b.co", "long enough pass"))
            .await
            .unwrap();
        assert!(svc.verify_access(&tokens.access).is_err());
    }
}
