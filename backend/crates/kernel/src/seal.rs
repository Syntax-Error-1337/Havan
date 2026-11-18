//! Secrets that have to be readable again, sealed at rest.
//!
//! A password is hashed, because nothing ever needs to read it back. A TOTP seed is the opposite: verifying a code means
//! recomputing it, so the server has to hold the secret itself. That makes the column a password in every way that
//! matters — anyone with a database dump can mint valid codes — so it is encrypted with the key that already exists for
//! exactly this job, `HAVEN_VAULT_KEY` (256 bits, from the environment, never in the database).
//!
//! AES-256-GCM, a fresh 96-bit nonce per seal, and the nonce travels in front of the ciphertext. A nonce reused under the
//! same key is the one way this construction fails catastrophically, so it is generated per call and never stored.
//!
//! The hex form of the key is decoded here rather than at the call site: one place to be wrong about it.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Nonce};
use base64::Engine;

use crate::config::Secret;
use crate::error::AppError;

/// 32 bytes, which GCM requires and the config already validates as 64 hex characters.
fn key_bytes(key: &Secret) -> Result<[u8; 32], AppError> {
    let raw = key.expose();
    let decoded: Vec<u8> = (0..raw.len() / 2)
        .map(|i| u8::from_str_radix(&raw[i * 2..i * 2 + 2], 16))
        .collect::<Result<_, _>>()
        .map_err(|_| AppError::Internal("vault key is not hex".into()))?;
    decoded
        .try_into()
        .map_err(|_| AppError::Internal("vault key is not 32 bytes".into()))
}

fn cipher(key: &Secret) -> Result<Aes256Gcm, AppError> {
    Aes256Gcm::new_from_slice(&key_bytes(key)?)
        .map_err(|_| AppError::Internal("vault key is not usable".into()))
}

/// Seal a secret for storage: `base64(nonce ‖ ciphertext ‖ tag)`.
pub fn seal(key: &Secret, plaintext: &[u8]) -> Result<String, AppError> {
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let sealed = cipher(key)?
        .encrypt(&nonce, plaintext)
        .map_err(|_| AppError::Internal("could not seal the secret".into()))?;

    let mut out = nonce.to_vec();
    out.extend_from_slice(&sealed);
    Ok(base64::engine::general_purpose::STANDARD.encode(out))
}

/// Open a sealed secret. A wrong key or a tampered row is an error, never a silent empty string: a TOTP seed that
/// "opens" to nothing would accept no codes and look like a person's mistake.
pub fn open(key: &Secret, sealed: &str) -> Result<Vec<u8>, AppError> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(sealed.trim())
        .map_err(|_| AppError::Internal("sealed secret is not base64".into()))?;
    if raw.len() < 12 {
        return Err(AppError::Internal("sealed secret is truncated".into()));
    }

    let (nonce, body) = raw.split_at(12);
    cipher(key)?
        .decrypt(Nonce::from_slice(nonce), body)
        .map_err(|_| AppError::Internal("sealed secret did not open".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(fill: &str) -> Secret {
        Secret::new(fill.repeat(32).chars().take(64).collect::<String>())
    }

    #[test]
    fn a_sealed_secret_opens_with_the_same_key_only() {
        let k = key("ab");
        let sealed = seal(&k, b"JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(open(&k, &sealed).unwrap(), b"JBSWY3DPEHPK3PXP");
        assert!(
            open(&key("cd"), &sealed).is_err(),
            "another key must not open it"
        );
    }

    #[test]
    fn the_same_plaintext_seals_differently_every_time() {
        // A fresh nonce per seal. Two identical ciphertexts in a column mean the nonce was reused, and a reused nonce
        // under GCM is how a key is recovered from ciphertext alone.
        let k = key("ab");
        let a = seal(&k, b"same").unwrap();
        let b = seal(&k, b"same").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn a_tampered_row_does_not_open() {
        let k = key("ab");
        let mut bytes = base64::engine::general_purpose::STANDARD
            .decode(seal(&k, b"secret").unwrap())
            .unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        let tampered = base64::engine::general_purpose::STANDARD.encode(bytes);
        assert!(
            open(&k, &tampered).is_err(),
            "GCM must refuse a modified tag"
        );
    }
}
