//! Vendor API keys.
//!
//! A vendor's factory tooling talks to the platform with a key, not with a
//! person's password. The rules that matter are about the secret: it is shown
//! once, stored as a digest, and revocable without touching the account.

use std::future::Future;

use kernel::AppError;
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// `hv_<8 hex prefix>_<32 hex secret>`. The prefix is the lookup key and is
/// stored in the clear; the secret never is.
pub const KEY_PREFIX: &str = "hv";

#[derive(Debug, Clone, Serialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub name: String,
    pub prefix: String,
    /// Only ever set on the response that creates the key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_used_at: Option<time::OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub revoked_at: Option<time::OffsetDateTime>,
}

/// A freshly minted key: what the vendor sees once, and what we keep.
#[derive(Debug, Clone)]
pub struct NewKey {
    pub id: Uuid,
    pub name: String,
    pub prefix: String,
    pub hash: String,
    pub secret: String,
}

pub fn mint(id: Uuid, name: &str) -> NewKey {
    // Both halves are random, and independently so. A prefix taken from the key
    // id's v7 bytes is a timestamp, so two keys minted in the same millisecond
    // shared one — and the prefix is UNIQUE, which made the second key a 500.
    // A prefix taken from the digest would work but leaks eight characters of it
    // into the index; there is no reason to couple them at all.
    let prefix = hex(&Uuid::new_v4().as_bytes()[..4]);
    let secret = hex(&Uuid::new_v4().as_bytes()[..16]);
    let hash = digest(&secret);

    NewKey {
        id,
        name: name.trim().to_string(),
        secret: format!("{KEY_PREFIX}_{prefix}_{secret}"),
        hash,
        prefix,
    }
}

/// Split a presented key into the part we look up and the part we verify.
pub fn parse(presented: &str) -> Option<(&str, &str)> {
    let rest = presented.strip_prefix(KEY_PREFIX)?.strip_prefix('_')?;
    let (prefix, secret) = rest.split_once('_')?;
    if prefix.len() != 8 || secret.len() != 32 {
        return None;
    }
    Some((prefix, secret))
}

/// SHA-256, not Argon2, and deliberately: a key is 128 bits of randomness, so
/// there is nothing to slow an attacker down to. Argon2 here would add ~50 ms to
/// every authenticated request and buy nothing. Passwords are still Argon2.
pub fn digest(secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hex(&hasher.finalize())
}

pub fn matches(presented_secret: &str, stored_hash: &str) -> bool {
    // Constant-time compare: an early return on the first differing byte is a
    // timing oracle on a secret the caller is guessing.
    let candidate = digest(presented_secret);
    if candidate.len() != stored_hash.len() {
        return false;
    }
    candidate
        .bytes()
        .zip(stored_hash.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub trait ApiKeyRepo {
    fn create(
        &self,
        vendor_id: Uuid,
        key: &NewKey,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    fn list(&self, vendor_id: Uuid) -> impl Future<Output = Result<Vec<ApiKey>, AppError>> + Send;

    fn revoke(
        &self,
        vendor_id: Uuid,
        id: Uuid,
    ) -> impl Future<Output = Result<(), AppError>> + Send;

    /// Resolve a presented key to its vendor. Cross-tenant by nature: the caller
    /// has no vendor yet — that is the question being answered.
    fn authenticate(
        &self,
        prefix: &str,
        secret: &str,
    ) -> impl Future<Output = Result<Option<Uuid>, AppError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minted_key_round_trips_through_parse() {
        let key = mint(Uuid::now_v7(), "factory tool");
        let (prefix, secret) = parse(&key.secret).expect("parses");
        assert_eq!(prefix, key.prefix);
        assert_eq!(digest(secret), key.hash);
        assert!(matches(secret, &key.hash));
    }

    #[test]
    fn two_keys_minted_in_the_same_instant_differ() {
        // The prefix column is UNIQUE. Deriving it from anything monotonic — a
        // v7 uuid's timestamp bytes — makes a burst of keys collide.
        let a = mint(Uuid::now_v7(), "a");
        let b = mint(Uuid::now_v7(), "b");
        assert_ne!(a.prefix, b.prefix);
        assert_ne!(a.secret, b.secret);
    }

    #[test]
    fn the_prefix_is_not_the_secret() {
        let key = mint(Uuid::now_v7(), "x");
        // Storing the prefix is safe only because it is not enough to use.
        assert!(!key.hash.contains(&key.prefix));
        assert_eq!(key.prefix.len(), 8);
        assert_eq!(key.secret.len(), 3 + 8 + 1 + 32);
    }

    #[test]
    fn nonsense_is_rejected_rather_than_looked_up() {
        for bad in [
            "",
            "hv_",
            "hv_abc_def",
            "nope_12345678_",
            "hv_12345678_short",
        ] {
            assert!(parse(bad).is_none(), "{bad} should not parse");
        }
    }

    #[test]
    fn a_different_secret_does_not_match() {
        let key = mint(Uuid::now_v7(), "x");
        let (_, secret) = parse(&key.secret).expect("parses");
        let other = mint(Uuid::now_v7(), "y");
        let (_, other_secret) = parse(&other.secret).expect("parses");

        assert!(matches(secret, &key.hash));
        assert!(!matches(other_secret, &key.hash));
    }
}
