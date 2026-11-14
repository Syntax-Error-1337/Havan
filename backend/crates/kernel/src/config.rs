//! Configuration from the environment. Every value has a documented default
//! except the secrets, which have none: a process that starts without its
//! signing keys must fail at boot, not at the first login.

use std::fmt;
use std::net::SocketAddr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Env {
    Dev,
    Test,
    Prod,
}

impl Env {
    pub fn is_prod(&self) -> bool {
        matches!(self, Env::Prod)
    }
}

#[derive(Clone)]
pub struct Config {
    pub env: Env,
    pub bind: SocketAddr,
    pub database_url: String,
    pub redis_url: String,
    pub mqtt_url: String,
    /// Where the device certificate authority is kept. On disk, not in a table: a CA
    /// private key in the database is one backup away from being everywhere.
    pub ca_dir: std::path::PathBuf,
    pub public_url: String,
    /// HMAC key for access tokens. ≥ 32 bytes.
    pub jwt_secret: Secret,
    /// AES-256-GCM key that seals device secrets at rest.
    pub vault_key: Secret,
    /// Ed25519 key that signs pairing tickets.
    pub ticket_secret: Secret,
    /// Optional token for `/ops/*`; when unset those routes are loopback-only.
    pub ops_token: Option<Secret>,
    pub db_max_connections: u32,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never print a secret, even by accident in a startup log line.
        f.debug_struct("Config")
            .field("env", &self.env)
            .field("bind", &self.bind)
            .field("database_url", &redact(&self.database_url))
            .field("redis_url", &redact(&self.redis_url))
            .field("mqtt_url", &self.mqtt_url)
            .field("public_url", &self.public_url)
            .field("jwt_secret", &"<redacted>")
            .field("vault_key", &"<redacted>")
            .field("ticket_secret", &"<redacted>")
            .field("ops_token", &self.ops_token.as_ref().map(|_| "<redacted>"))
            .field("db_max_connections", &self.db_max_connections)
            .finish()
    }
}

fn redact(url: &str) -> String {
    // postgres://user:pass@host/db → postgres://user:***@host/db
    match (url.find("://"), url.find('@')) {
        (Some(scheme_end), Some(at)) if scheme_end + 3 < at => {
            let creds = &url[scheme_end + 3..at];
            match creds.find(':') {
                Some(c) => format!(
                    "{}{}:***{}",
                    &url[..scheme_end + 3],
                    &creds[..c],
                    &url[at..]
                ),
                None => url.to_string(),
            }
        }
        _ => url.to_string(),
    }
}

/// A string that prints as `<redacted>` and compares in constant-ish time only
/// where it matters (token verification lives in `domain::identity`).
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(s: impl Into<String>) -> Self {
        Secret(s.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    #[error("{0} must be at least {1} bytes, got {2}")]
    TooShort(&'static str, usize, usize),
    #[error("{0} is not a valid socket address: {1}")]
    BadAddr(&'static str, String),
    #[error("{0} is not valid hex (expected {1} hex chars)")]
    BadHex(&'static str, usize),
}

pub const MIN_SECRET_BYTES: usize = 32;

/// Reads configuration from anywhere — the environment in production, a fixed
/// map in tests. Without this seam a config test can only pass on a machine
/// that happens to have the wrong variables set.
type Lookup<'a> = dyn Fn(&str) -> Option<String> + 'a;

fn var(get: &Lookup<'_>, name: &'static str) -> Result<String, ConfigError> {
    get(name)
        .filter(|v| !v.trim().is_empty())
        .ok_or(ConfigError::Missing(name))
}

fn var_or(get: &Lookup<'_>, name: &'static str, default: &str) -> String {
    get(name)
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(&|k| std::env::var(k).ok())
    }

    pub fn from_lookup(get: &Lookup<'_>) -> Result<Self, ConfigError> {
        let env = match var_or(get, "HAVEN_ENV", "dev").as_str() {
            "prod" | "production" => Env::Prod,
            "test" => Env::Test,
            _ => Env::Dev,
        };

        let ca_dir = std::path::PathBuf::from(var_or(get, "HAVEN_CA_DIR", ".haven/ca"));
        let bind_raw = var_or(get, "HAVEN_BIND", "127.0.0.1:8080");
        let bind: SocketAddr = bind_raw
            .parse()
            .map_err(|_| ConfigError::BadAddr("HAVEN_BIND", bind_raw.clone()))?;

        let jwt_secret = Secret::new(var(get, "HAVEN_JWT_SECRET")?);
        if jwt_secret.len() < MIN_SECRET_BYTES {
            return Err(ConfigError::TooShort(
                "HAVEN_JWT_SECRET",
                MIN_SECRET_BYTES,
                jwt_secret.len(),
            ));
        }

        let vault_raw = var(get, "HAVEN_VAULT_KEY")?;
        if vault_raw.len() != 64 || !vault_raw.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ConfigError::BadHex("HAVEN_VAULT_KEY", 64));
        }

        let ticket_secret = Secret::new(var(get, "HAVEN_TICKET_SECRET")?);
        if ticket_secret.len() < MIN_SECRET_BYTES {
            return Err(ConfigError::TooShort(
                "HAVEN_TICKET_SECRET",
                MIN_SECRET_BYTES,
                ticket_secret.len(),
            ));
        }

        Ok(Config {
            env,
            bind,
            ca_dir,
            database_url: var_or(
                get,
                "DATABASE_URL",
                "postgres://haven:haven@localhost:5432/haven",
            ),
            redis_url: var_or(get, "REDIS_URL", "redis://localhost:6379"),
            mqtt_url: var_or(get, "MQTT_URL", "mqtt://localhost:1883"),
            public_url: var_or(get, "HAVEN_PUBLIC_URL", "http://localhost:8080"),
            jwt_secret,
            vault_key: Secret::new(vault_raw),
            ticket_secret,
            ops_token: get("OPS_TOKEN").filter(|v| !v.is_empty()).map(Secret::new),
            db_max_connections: var_or(get, "HAVEN_DB_POOL", "16")
                .parse()
                .unwrap_or(16)
                .clamp(1, 256),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_a_secret() {
        let cfg = Config {
            env: Env::Dev,
            bind: "127.0.0.1:8080".parse().unwrap(),
            ca_dir: std::path::PathBuf::from(".haven/ca"),
            database_url: "postgres://u:p@h/db".into(),
            redis_url: "redis://h".into(),
            mqtt_url: "mqtt://h".into(),
            public_url: "http://h".into(),
            jwt_secret: Secret::new("super-secret-value"),
            vault_key: Secret::new("0".repeat(64)),
            ticket_secret: Secret::new("another-secret"),
            ops_token: Some(Secret::new("ops")),
            db_max_connections: 4,
        };
        let dbg = format!("{cfg:?}");
        assert!(!dbg.contains("super-secret-value"));
        assert!(!dbg.contains("another-secret"));
        assert!(dbg.contains("u:***@h"));
    }

    #[test]
    fn a_missing_secret_names_the_variable() {
        // A lookup that knows nothing: the same shape as a fresh container.
        let err = Config::from_lookup(&|_| None).unwrap_err();
        assert_eq!(err, ConfigError::Missing("HAVEN_JWT_SECRET"));
    }

    #[test]
    fn a_short_secret_is_refused_with_its_length() {
        let err = Config::from_lookup(&|k| match k {
            "HAVEN_JWT_SECRET" => Some("short".to_string()),
            _ => None,
        })
        .unwrap_err();
        assert_eq!(err, ConfigError::TooShort("HAVEN_JWT_SECRET", 32, 5));
    }

    #[test]
    fn a_non_hex_vault_key_is_refused() {
        let err = Config::from_lookup(&|k| match k {
            "HAVEN_JWT_SECRET" => Some("x".repeat(32)),
            "HAVEN_VAULT_KEY" => Some("zz".to_string()),
            _ => None,
        })
        .unwrap_err();
        assert_eq!(err, ConfigError::BadHex("HAVEN_VAULT_KEY", 64));
    }
}
