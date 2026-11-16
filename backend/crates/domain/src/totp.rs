//! Time-based one-time passwords, for the second factor in the portal.
//!
//! Deliberately small and boring: HMAC-SHA1 over a 30-second counter, six digits, one step of drift
//! in each direction. Everything else — which members must present a code, when the policy may be
//! switched on — belongs to the vendor's own security policy, not here.
//!
//! Base32 is implemented rather than pulled in because it is thirty lines and a dependency that
//! exists to be audited; the algorithm is RFC 4648 with no padding, which is what every
//! authenticator app accepts in an `otpauth://` URI.

use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// Seconds per code. The value every authenticator app assumes when the URI does not say.
pub const PERIOD: u64 = 30;
/// Digits in a code.
pub const DIGITS: u32 = 6;
/// One step either side, so a phone whose clock is slightly off still works. Two steps is the usual
/// ceiling; beyond that a code that was shoulder-surfed has a long life.
pub const DRIFT_STEPS: i64 = 1;

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// A fresh secret: 160 bits, the length RFC 4226 recommends.
pub fn new_secret() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 20];
    rand::thread_rng().fill_bytes(&mut bytes);
    base32_encode(&bytes)
}

pub fn base32_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut buffer: u32 = 0;
    let mut bits = 0;

    for byte in bytes {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            let index = ((buffer >> (bits - 5)) & 0x1f) as usize;
            out.push(ALPHABET[index] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        let index = ((buffer << (5 - bits)) & 0x1f) as usize;
        out.push(ALPHABET[index] as char);
    }
    out
}

pub fn base32_decode(input: &str) -> Option<Vec<u8>> {
    let mut buffer: u32 = 0;
    let mut bits = 0;
    let mut out = Vec::with_capacity(input.len() * 5 / 8);

    for c in input.chars() {
        let c = c.to_ascii_uppercase();
        if c == '=' || c == ' ' {
            continue;
        }
        let value = ALPHABET.iter().position(|a| *a as char == c)? as u32;
        buffer = (buffer << 5) | value;
        bits += 5;
        if bits >= 8 {
            out.push(((buffer >> (bits - 8)) & 0xff) as u8);
            bits -= 8;
        }
    }
    Some(out)
}

/// The code for a given counter value, as RFC 4226 defines it.
pub fn code_at(secret: &[u8], counter: u64) -> String {
    let mut mac = HmacSha1::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();

    // Dynamic truncation: the low nibble of the last byte picks the four bytes to use.
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let binary = (u32::from(digest[offset]) & 0x7f) << 24
        | u32::from(digest[offset + 1]) << 16
        | u32::from(digest[offset + 2]) << 8
        | u32::from(digest[offset + 3]);

    let modulo = 10u32.pow(DIGITS);
    format!("{:0width$}", binary % modulo, width = DIGITS as usize)
}

/// The counter for an instant.
pub fn counter_at(unix_seconds: i64) -> u64 {
    (unix_seconds.max(0) as u64) / PERIOD
}

/// What a phone would be showing right now.
pub fn current(secret_b32: &str, unix_seconds: i64) -> Option<String> {
    let secret = base32_decode(secret_b32)?;
    Some(code_at(&secret, counter_at(unix_seconds)))
}

/// Does this code belong to this secret, within the drift window?
///
/// Compared in constant time and over every candidate step, so the answer does not depend on
/// whether the code was wrong or *nearly* right.
pub fn verify(secret_b32: &str, code: &str, unix_seconds: i64) -> bool {
    let Some(secret) = base32_decode(secret_b32) else {
        return false;
    };
    let code = code.trim();
    if code.len() != DIGITS as usize || !code.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    let now = counter_at(unix_seconds) as i64;
    let mut matched = false;
    for step in -DRIFT_STEPS..=DRIFT_STEPS {
        let counter = now + step;
        if counter < 0 {
            continue;
        }
        let candidate = code_at(&secret, counter as u64);
        matched |= constant_time_eq(candidate.as_bytes(), code.as_bytes());
    }
    matched
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

/// The `otpauth://` URI an authenticator app scans.
pub fn enrolment_uri(issuer: &str, account: &str, secret_b32: &str) -> String {
    // The label is `issuer:account`, and both parts are percent-encoded — a vendor email with a `+`
    // or an issuer with a space otherwise produces a QR code that silently enrols the wrong label.
    let label = format!("{issuer}:{account}");
    format!(
        "otpauth://totp/{}?secret={}&issuer={}&algorithm=SHA1&digits={}&period={}",
        percent_encode(&label),
        secret_b32,
        percent_encode(issuer),
        DIGITS,
        PERIOD
    )
}

fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The secret from RFC 6238's appendix, as base32.
    const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn base32_round_trips() {
        for length in [0usize, 1, 5, 10, 20, 21] {
            let bytes: Vec<u8> = (0..length).map(|i| (i as u8).wrapping_mul(37)).collect();
            let encoded = base32_encode(&bytes);
            assert_eq!(
                base32_decode(&encoded).expect("decode"),
                bytes,
                "length {length}"
            );
        }
    }

    #[test]
    fn base32_rejects_a_character_it_does_not_have() {
        assert!(base32_decode("ABC1DEF").is_none());
        // Padding and whitespace are tolerated, which is what a person pasting a secret does.
        assert!(base32_decode("MZXW 6===").is_some());
    }

    #[test]
    fn matches_the_rfc_vectors() {
        // RFC 6238 appendix B, truncated to six digits.
        let cases = [
            (59, "287082"),
            (1_111_111_109, "081804"),
            (1_111_111_111, "050471"),
            (1_234_567_890, "005924"),
            (2_000_000_000, "279037"),
        ];
        for (seconds, expected) in cases {
            assert_eq!(
                current(RFC_SECRET, seconds).expect("code"),
                expected,
                "at {seconds}"
            );
        }
    }

    #[test]
    fn a_code_works_one_step_either_side_and_no_further() {
        let now = 1_111_111_111;
        let previous = current(RFC_SECRET, now - PERIOD as i64).expect("code");
        let next = current(RFC_SECRET, now + PERIOD as i64).expect("code");
        let too_old = current(RFC_SECRET, now - 2 * PERIOD as i64).expect("code");

        assert!(verify(RFC_SECRET, &previous, now), "a slow phone must work");
        assert!(verify(RFC_SECRET, &next, now), "a fast phone must work");
        assert!(
            !verify(RFC_SECRET, &too_old, now),
            "two steps old is a code that had time to leak"
        );
    }

    #[test]
    fn rubbish_is_not_a_code() {
        let now = 1_111_111_111;
        for bad in ["", "12345", "1234567", "abcdef", "  "].iter() {
            assert!(!verify(RFC_SECRET, bad, now), "{bad:?} was accepted");
        }
        assert!(!verify("not-base32!", "123456", now));
    }

    #[test]
    fn the_enrolment_uri_is_what_an_authenticator_reads() {
        let uri = enrolment_uri("Haven", "dev+x@acme.test", "MZXW6YTBOI");
        assert!(uri.starts_with("otpauth://totp/Haven%3Adev%2Bx%40acme.test?secret=MZXW6YTBOI"));
        assert!(uri.contains("issuer=Haven"));
        assert!(uri.contains("digits=6"));
        assert!(uri.contains("period=30"));
    }

    #[test]
    fn a_fresh_secret_is_twenty_bytes_of_base32() {
        let secret = new_secret();
        assert_eq!(secret.len(), 32, "20 bytes is 32 base32 characters");
        assert_eq!(base32_decode(&secret).expect("decode").len(), 20);
        assert_ne!(
            secret,
            new_secret(),
            "two enrolments must not share a secret"
        );
    }
}
