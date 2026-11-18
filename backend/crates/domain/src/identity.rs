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

use crate::totp as domain_totp;
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

/// What the account's second factor is doing, in the three states a screen can render.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TotpState {
    Off,
    /// A seed exists and the person has not yet proved their app produces the right codes.
    Pending,
    On,
}

/// The one moment the seed can be handed over: at enrolment, before it is ever used to check a sign-in.
pub struct StartedTotp {
    pub secret: String,
    pub uri: String,
}

/// One TOTP enrolment: the sealed seed, whether the person proved their authenticator works, and the last step a code
/// was accepted for.
#[derive(Debug, Clone)]
pub struct UserTotp {
    pub sealed_secret: String,
    pub confirmed_at: Option<time::OffsetDateTime>,
    pub last_step: Option<i64>,
}

impl UserTotp {
    pub fn is_confirmed(&self) -> bool {
        self.confirmed_at.is_some()
    }
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
/// One sign-in, as the settings screen sees it.
///
/// No device name, no IP, no location: this service does not store them, and inventing a "Chrome on macOS" from a
/// user-agent it never recorded would be a guess dressed as a fact. What it does know is when the session started, when
/// it was last refreshed, and when it will expire on its own.
#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub family_id: Uuid,
    /// When this sign-in happened.
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: time::OffsetDateTime,
    /// The most recent token rotation — a proxy for "last used".
    #[serde(with = "time::serde::rfc3339")]
    pub last_seen_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: time::OffsetDateTime,
}

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

    /// Every family of refresh tokens that is still alive, one row per sign-in.
    ///
    /// A family is one sign-in on one device: the refresh token rotates, the family does not. This is what a "signed-in
    /// devices" list is made of — and until now the app could not tell a customer their account was open on a phone they
    /// no longer have.
    fn active_sessions(
        &self,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Vec<Session>, AppError>> + Send;

    /// How many sign-ins are alive, without listing them. The list is capped at twenty; this is the number the screen
    /// prints, because "856 devices" and "20 devices" are different answers to the same question.
    fn active_sessions_count(
        &self,
        user_id: Uuid,
    ) -> impl Future<Output = Result<i64, AppError>> + Send;

    /// Revoke every live family except `keep` — "sign out everywhere else".
    ///
    /// `keep` is `None` for "sign out everywhere", which includes this device: the customer does that when they think
    /// the account is compromised, and leaving their own session alive would be the one they cannot see.
    fn revoke_all_except(
        &self,
        user_id: Uuid,
        keep: Option<Uuid>,
    ) -> impl Future<Output = Result<u64, AppError>> + Send;

    /// Replace the stored password hash.
    fn set_password_hash(
        &self,
        user_id: Uuid,
        hash: &str,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Store (or replace) an enrolment that has not been confirmed yet.
    fn start_totp(
        &self,
        user_id: Uuid,
        sealed_secret: &str,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// The enrolment, sealed. `None` when the person has never turned the second factor on.
    fn totp(
        &self,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Option<UserTotp>, AppError>> + Send;

    /// Mark the enrolment proved. Only a confirmed enrolment gates a sign-in.
    fn confirm_totp(&self, user_id: Uuid) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Record the step a code was accepted for, refusing a repeat.
    ///
    /// Returns `false` when that step has already been spent — the difference between "valid code" and "valid code that
    /// somebody is reusing", which is the whole reason the column exists.
    fn spend_totp_step(
        &self,
        user_id: Uuid,
        step: i64,
    ) -> impl Future<Output = Result<bool, AppError>> + Send;

    fn delete_totp(&self, user_id: Uuid) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Record that this person signed in. Best effort: a login must not fail because a
    /// timestamp could not be written.
    fn touch_vendor_login(
        &self,
        user_id: Uuid,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// The member's second factor, if any: the secret and whether they have proved they can read a
    /// code from it.
    fn vendor_totp(
        &self,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Option<(String, bool)>, AppError>> + Send;

    fn find_vendor_user(
        &self,
        vendor_id: Uuid,
        email: &str,
    ) -> impl Future<Output = Result<Option<VendorUser>, AppError>> + Send;

    /// A vendor's id from the short name the operator writes down (`acme`), for the portal's door.
    fn vendor_id_by_slug(
        &self,
        slug: &str,
    ) -> impl Future<Output = Result<Option<Uuid>, AppError>> + Send;
}

pub struct IdentityService<R> {
    repo: Arc<R>,
    encoding: EncodingKey,
    decoding: DecodingKey,
    /// For the one secret that has to be read back: a TOTP seed. Absent in tests that never touch it, which is why the
    /// methods that need it say so rather than defaulting to an unsealed column.
    vault_key: Option<kernel::config::Secret>,
}

impl<R: IdentityRepo + Send + Sync> IdentityService<R> {
    pub fn new(repo: Arc<R>, secret: &str) -> Self {
        Self {
            repo,
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
            vault_key: None,
        }
    }

    /// The second factor is unusable without the vault key, so it is wired in explicitly rather than assumed.
    pub fn with_vault_key(mut self, key: kernel::config::Secret) -> Self {
        self.vault_key = Some(key);
        self
    }

    fn vault(&self) -> Result<&kernel::config::Secret, AppError> {
        self.vault_key
            .as_ref()
            .ok_or_else(|| AppError::Internal("the vault key is not configured".into()))
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

    /// Sign in, with the second factor when the account has one.
    ///
    /// The password is checked first and the code second, so a wrong code on a wrong password still says "wrong password":
    /// there is nothing to gain from telling an attacker which half they got right. When the code is missing the answer
    /// is `second_factor_required` rather than a generic refusal, because "wrong password" would send somebody who has a
    /// working password to reset it.
    pub async fn login_with_code(
        &self,
        email: &str,
        password: &str,
        code: Option<&str>,
    ) -> Result<Tokens, AppError> {
        let email = normalize_email(email);
        let user = self.repo.find_by_email(&email).await?;

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

        if let Some(enrolment) = self.repo.totp(user.id).await?
            && enrolment.is_confirmed()
        {
            let Some(code) = code else {
                return Err(AppError::code(
                    ErrorCode::SecondFactorRequired,
                    "this account has a second factor: send the code from your authenticator",
                ));
            };
            self.check_second_factor(user.id, &enrolment, code).await?;
        }

        self.issue_session(user.id).await
    }

    /// The second factor as a person sees it: on, half-on, or off.
    pub async fn totp_state(&self, user_id: Uuid) -> Result<TotpState, AppError> {
        Ok(match self.repo.totp(user_id).await? {
            Some(row) if row.is_confirmed() => TotpState::On,
            Some(_) => TotpState::Pending,
            None => TotpState::Off,
        })
    }

    /// Begin an enrolment: a fresh seed, stored sealed and unconfirmed, plus the URI the authenticator app scans.
    ///
    /// The seed is returned to the caller because it is the one time it can be — after this the server will only ever
    /// answer codes. It is not a confirmation: until the person proves a code works, a sign-in is not gated, because a
    /// second factor nobody has set up is a way to lock yourself out of your own house.
    pub async fn start_totp(&self, user_id: Uuid, account: &str) -> Result<StartedTotp, AppError> {
        let secret = domain_totp::new_secret();
        self.repo
            .start_totp(
                user_id,
                &kernel::seal::seal(self.vault()?, secret.as_bytes())?,
            )
            .await?;

        Ok(StartedTotp {
            secret: secret.clone(),
            uri: domain_totp::enrolment_uri("Haven", account, &secret),
        })
    }

    pub async fn confirm_totp(&self, user_id: Uuid, code: &str) -> Result<(), AppError> {
        let Some(enrolment) = self.repo.totp(user_id).await? else {
            return Err(AppError::code(
                ErrorCode::Conflict,
                "start an enrolment first: there is no secret to confirm",
            ));
        };
        self.check_second_factor(user_id, &enrolment, code).await?;
        self.repo.confirm_totp(user_id).await
    }

    pub async fn disable_totp(&self, user_id: Uuid) -> Result<(), AppError> {
        self.repo.delete_totp(user_id).await
    }

    /// Verify a code and spend its 30-second step.
    ///
    /// The step is recorded because a code stays valid for one step either side of now: without it, six digits seen over a
    /// shoulder keep working for ninety seconds after they were used.
    async fn check_second_factor(
        &self,
        user_id: Uuid,
        enrolment: &UserTotp,
        code: &str,
    ) -> Result<(), AppError> {
        let secret = kernel::seal::open(self.vault()?, &enrolment.sealed_secret)?;
        let secret = String::from_utf8(secret)
            .map_err(|_| AppError::Internal("the stored TOTP secret is not text".into()))?;
        let now = kernel::now().unix_timestamp();

        let code = code.trim().replace(' ', "");
        // The step the code belongs to, not the step the clock is on: recording "now" would refuse a good code from the
        // next step (the drift window allows it) and would accept a code twice if the clock had moved on.
        let Some(step) = domain_totp::matching_step(&secret, &code, now) else {
            return Err(AppError::code(
                ErrorCode::Unauthorized,
                "that code does not match; check the clock on your phone",
            ));
        };
        if !self.repo.spend_totp_step(user_id, step).await? {
            return Err(AppError::code(
                ErrorCode::Unauthorized,
                "that code has already been used; wait for the next one",
            ));
        }

        Ok(())
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

    /// Turns what a person types into a vendor id.
    ///
    /// The portal used to ask for a **UUID**: thirty-six characters, dashes, no meaning, and no way to
    /// remember or check it. The vendor table has carried a `slug` since the first migration — `acme`,
    /// the short name the operator writes in their own notes — so the door accepts either, and the field
    /// on screen asks for a name.
    ///
    /// A workspace that does not exist and a wrong password produce the same error: announcing "no such
    /// workspace" would let anyone enumerate the platform's customers.
    pub async fn resolve_vendor(&self, vendor: &str) -> Result<Uuid, AppError> {
        let typed = vendor.trim();
        if let Ok(id) = Uuid::parse_str(typed) {
            return Ok(id);
        }
        let slug = typed.to_lowercase();
        self.repo
            .vendor_id_by_slug(&slug)
            .await?
            .ok_or_else(invalid_credentials)
    }

    /// Portal login. The customer's `login` is a different door with a
    /// different table, on purpose.
    pub async fn login_vendor(
        &self,
        vendor_id: Uuid,
        email: &str,
        password: &str,
        code: Option<&str>,
    ) -> Result<Tokens, AppError> {
        let email = normalize_email(email);
        let Some(user) = self.repo.find_vendor_user(vendor_id, &email).await? else {
            let _ = verify_password(password, dummy_hash());
            return Err(invalid_credentials());
        };
        if !verify_password(password, &user.password_hash) {
            return Err(invalid_credentials());
        }

        // The second factor, checked after the password and before any token exists. Order matters:
        // answering "wrong code" to a caller who does not have the password would confirm the
        // account exists.
        // A secret that was never confirmed is not a factor: it is an enrolment somebody abandoned,
        // and treating it as one would lock them out of an account they can still use.
        if let Some((secret, true)) = self.repo.vendor_totp(user.id).await? {
            require_second_factor(&secret, code)?;
        }

        // The portal shows "last login", and a column nothing ever writes shows "never" forever.
        if let Err(e) = self.repo.touch_vendor_login(user.id).await {
            tracing::warn!(error = %e, "could not record the vendor login time");
        }

        self.issue_refresh(None, Some(user.id), kernel::new_id(), Some(user.vendor_id))
            .await
    }

    /// Every sign-in still alive for this person, newest first, capped at twenty.
    pub async fn sessions(&self, user_id: Uuid) -> Result<(i64, Vec<Session>), AppError> {
        let total = self.repo.active_sessions_count(user_id).await?;
        let list = self.repo.active_sessions(user_id).await?;
        Ok((total, list))
    }

    /// Change a password, having proved the old one.
    ///
    /// Every other session is ended: a password change is the one moment a customer can act on "somebody else may have
    /// this", and leaving the session that the other person holds alive would make the new password theatre. The device
    /// that made the change keeps its session — it just proved the password twice.
    pub async fn change_password(
        &self,
        user_id: Uuid,
        current: &str,
        next: &str,
        keep_refresh_token: Option<&str>,
    ) -> Result<u64, AppError> {
        let Some(user) = self.repo.find_by_id(user_id).await? else {
            return Err(invalid_credentials());
        };
        let Some(hash) = user.password_hash.as_deref() else {
            return Err(AppError::validation(
                "this account has no password to change",
            ));
        };
        if !verify_password(current, hash) {
            return Err(invalid_credentials());
        }
        if verify_password(next, hash) {
            return Err(AppError::validation("the new password is the current one"));
        }
        validate_password(next)?;

        self.repo
            .set_password_hash(user_id, &hash_password(next)?)
            .await?;

        // Keep the caller's own family, if it named one: signing the person out of the phone they just changed the
        // password on is a punishment for doing the right thing.
        let keep = match keep_refresh_token.map(split_refresh_token) {
            Some(Ok((id, _))) => match self.repo.find_refresh(id).await? {
                Some(row) if row.user_id == Some(user_id) => Some(row.family_id),
                _ => None,
            },
            _ => None,
        };

        self.repo.revoke_all_except(user_id, keep).await
    }

    /// Sign out everywhere except this device.
    ///
    /// The refresh token is what names "this device": it carries the family id, so the caller does not have to send an
    /// id it could guess. A malformed or unknown token revokes nothing rather than everything — a typo must not be able
    /// to end sessions the customer is still using.
    pub async fn logout_others(&self, user_id: Uuid, refresh_token: &str) -> Result<u64, AppError> {
        let keep = match split_refresh_token(refresh_token) {
            Ok((id, _)) => match self.repo.find_refresh(id).await? {
                Some(row) if row.user_id == Some(user_id) => Some(row.family_id),
                _ => return Ok(0),
            },
            Err(_) => return Ok(0),
        };
        self.repo.revoke_all_except(user_id, keep).await
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

/// The second factor, or a refusal that says which one.
///
/// A missing code and a wrong code are different answers: the first means "ask me for one", the
/// second means "try again". Collapsing them makes the portal show a password error to a person who
/// typed the password correctly.
fn require_second_factor(secret: &str, code: Option<&str>) -> Result<(), AppError> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();

    let (message, state) = match code {
        Some(code) if domain_totp::verify(secret, code, now) => return Ok(()),
        Some(_) => (
            "that code is not valid; check the clock on your phone",
            "invalid",
        ),
        None => ("this account needs a two-factor code", "required"),
    };

    let mut err = AppError::code(kernel::ErrorCode::Unauthorized, message);
    if let AppError::Coded { details, .. } = &mut err {
        details.insert("totp".into(), state.to_string());
    }
    Err(err)
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
        /// The vendor's second factor: `(secret, confirmed)`. Defaults to none, which is what most
        /// of these tests exercise.
        totp: Mutex<Option<(String, bool)>>,
        /// The customer's own second factor. Kept apart from the vendor one: they are different principals, and a test
        /// that confused them would pass while the portal and the app checked each other's rows.
        user_totp: Mutex<Option<UserTotp>>,
        /// The portal account, if a test needs one to exist.
        vendor_user: Mutex<Option<VendorUser>>,
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
        async fn touch_vendor_login(&self, _user_id: Uuid) -> Result<(), AppError> {
            Ok(())
        }

        /// The one workspace this fake knows about: `acme`.
        async fn vendor_id_by_slug(&self, slug: &str) -> Result<Option<Uuid>, AppError> {
            Ok((slug == "acme")
                .then(|| {
                    self.vendor_user
                        .lock()
                        .unwrap()
                        .as_ref()
                        .map(|u| u.vendor_id)
                })
                .flatten())
        }

        async fn vendor_totp(&self, _user_id: Uuid) -> Result<Option<(String, bool)>, AppError> {
            Ok(self.totp.lock().unwrap().clone())
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
            Ok(self.vendor_user.lock().unwrap().clone())
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

        async fn active_sessions(&self, user_id: Uuid) -> Result<Vec<Session>, AppError> {
            // Same rule as the SQL: one row per family, alive only, newest first. A fake that returns rows the real
            // query hides is a fake that passes the test the bug lives in.
            let now = kernel::now();
            let mut seen: Vec<Session> = Vec::new();
            for row in self.refresh.lock().unwrap().iter() {
                if row.user_id != Some(user_id) || row.revoked_at.is_some() || row.expires_at <= now
                {
                    continue;
                }
                if seen.iter().any(|s| s.family_id == row.family_id) {
                    continue;
                }
                seen.push(Session {
                    family_id: row.family_id,
                    started_at: now,
                    last_seen_at: row.used_at.unwrap_or(now),
                    expires_at: row.expires_at,
                });
            }
            Ok(seen)
        }

        async fn start_totp(&self, _user_id: Uuid, sealed_secret: &str) -> Result<(), AppError> {
            *self.user_totp.lock().unwrap() = Some(UserTotp {
                sealed_secret: sealed_secret.to_string(),
                confirmed_at: None,
                last_step: None,
            });
            Ok(())
        }

        async fn totp(&self, _user_id: Uuid) -> Result<Option<UserTotp>, AppError> {
            Ok(self.user_totp.lock().unwrap().clone())
        }

        async fn confirm_totp(&self, _user_id: Uuid) -> Result<(), AppError> {
            let mut row = self.user_totp.lock().unwrap();
            let Some(enrolment) = row.as_mut() else {
                return Err(AppError::not_found("enrolment"));
            };
            enrolment.confirmed_at = Some(kernel::now());
            Ok(())
        }

        async fn spend_totp_step(&self, _user_id: Uuid, step: i64) -> Result<bool, AppError> {
            let mut row = self.user_totp.lock().unwrap();
            let Some(enrolment) = row.as_mut() else {
                return Ok(false);
            };
            if enrolment.last_step.is_some_and(|last| last >= step) {
                return Ok(false);
            }
            enrolment.last_step = Some(step);
            Ok(true)
        }

        async fn delete_totp(&self, _user_id: Uuid) -> Result<(), AppError> {
            *self.user_totp.lock().unwrap() = None;
            Ok(())
        }

        async fn set_password_hash(&self, user_id: Uuid, hash: &str) -> Result<(), AppError> {
            let mut users = self.users.lock().unwrap();
            let Some(user) = users.iter_mut().find(|u| u.id == user_id) else {
                return Err(AppError::not_found("account"));
            };
            user.password_hash = Some(hash.to_string());
            Ok(())
        }

        async fn active_sessions_count(&self, user_id: Uuid) -> Result<i64, AppError> {
            let now = kernel::now();
            let mut families: Vec<Uuid> = Vec::new();
            for row in self.refresh.lock().unwrap().iter() {
                if row.user_id == Some(user_id)
                    && row.revoked_at.is_none()
                    && row.expires_at > now
                    && !families.contains(&row.family_id)
                {
                    families.push(row.family_id);
                }
            }
            Ok(families.len() as i64)
        }

        async fn revoke_all_except(
            &self,
            user_id: Uuid,
            keep: Option<Uuid>,
        ) -> Result<u64, AppError> {
            let now = kernel::now();
            let mut n = 0;
            for row in self.refresh.lock().unwrap().iter_mut() {
                if row.user_id == Some(user_id)
                    && row.revoked_at.is_none()
                    && keep.is_none_or(|k| row.family_id != k)
                {
                    row.revoked_at = Some(now);
                    n += 1;
                }
            }
            Ok(n)
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

    /// A portal account whose password is known, so the second factor is the only variable.
    fn portal_account(svc: &IdentityService<FakeRepo>, totp: Option<(String, bool)>) -> VendorUser {
        let user = VendorUser {
            id: Uuid::now_v7(),
            vendor_id: Uuid::now_v7(),
            email: "dev@acme.test".into(),
            password_hash: hash_password("long enough pass").expect("hash"),
            role: "owner".into(),
        };
        *svc.repo.vendor_user.lock().unwrap() = Some(user.clone());
        *svc.repo.totp.lock().unwrap() = totp;
        user
    }

    #[tokio::test]
    async fn a_confirmed_second_factor_is_required_and_must_be_right() {
        let svc = service();
        let secret = crate::totp::new_secret();
        let user = portal_account(&svc, Some((secret.clone(), true)));

        // No code at all: refused, and told why, so the portal can ask for one.
        let err = svc
            .login_vendor(user.vendor_id, "dev@acme.test", "long enough pass", None)
            .await
            .unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::Unauthorized);
        match &err {
            AppError::Coded { details, .. } => {
                assert_eq!(details.get("totp").map(String::as_str), Some("required"))
            }
            _ => panic!("expected a coded error"),
        }

        // A wrong code is refused too — and it is refused *after* the password, so it cannot be
        // used to discover which accounts exist.
        let err = svc
            .login_vendor(
                user.vendor_id,
                "dev@acme.test",
                "long enough pass",
                Some("000000"),
            )
            .await
            .unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::Unauthorized);

        // The right one works.
        let code = crate::totp::current(&secret, time::OffsetDateTime::now_utc().unix_timestamp())
            .expect("code");
        svc.login_vendor(
            user.vendor_id,
            "dev@acme.test",
            "long enough pass",
            Some(&code),
        )
        .await
        .expect("a valid code signs in");
    }

    #[tokio::test]
    async fn the_portal_door_accepts_a_workspace_name_or_its_id() {
        let svc = service();
        let user = portal_account(&svc, None);

        // What a person can type: the short name.
        assert_eq!(
            svc.resolve_vendor("acme").await.expect("slug resolves"),
            user.vendor_id
        );
        // What a script or an older client sends.
        assert_eq!(
            svc.resolve_vendor(&user.vendor_id.to_string())
                .await
                .expect("id resolves"),
            user.vendor_id
        );
        // A workspace that is not there is the same answer as a wrong password: naming the failure
        // would let anyone enumerate the platform's customers.
        let err = svc.resolve_vendor("not-a-vendor").await.unwrap_err();
        assert_eq!(err.code_kind(), ErrorCode::InvalidCredentials);
    }

    #[tokio::test]
    async fn an_unconfirmed_secret_is_not_a_factor() {
        let svc = service();
        // Abandoned halfway through enrolment: the secret exists, nobody proved they can read it.
        portal_account(&svc, Some((crate::totp::new_secret(), false)));

        svc.login_vendor(Uuid::nil(), "dev@acme.test", "long enough pass", None)
            .await
            .expect("an unfinished enrolment must not lock somebody out");
    }

    #[tokio::test]
    async fn an_account_without_a_second_factor_signs_in_as_before() {
        let svc = service();
        portal_account(&svc, None);

        svc.login_vendor(Uuid::nil(), "dev@acme.test", "long enough pass", None)
            .await
            .expect("no factor, no code required");
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
