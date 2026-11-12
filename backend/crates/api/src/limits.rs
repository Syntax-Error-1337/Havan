//! Who is calling, and how many times they have got it wrong.
//!
//! The limiter counts **failures**, not requests. A login limit that throttles successful
//! logins punishes the wrong person: someone with a script and someone with fat fingers
//! look identical to a request counter, and only one of them is a problem. Counting the
//! refusals means a legitimate user never notices the limit exists, while a password
//! spray runs out of attempts in ten tries.

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;

use crate::app::AppState;
use crate::error::ApiError;
use kernel::AppError;

/// The caller's address, when the server was told to record one.
///
/// `Option<ConnectInfo<..>>` is not an extractor in axum 0.8, and requiring
/// `ConnectInfo` outright would panic in tests and in any embedding that serves without
/// connect info. This extractor is total: it yields `None`, and callers treat that as an
/// unknown source rather than as a trusted one.
pub struct Peer(pub Option<IpAddr>);

impl FromRequestParts<AppState> for Peer {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Peer(
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|c| c.0.ip()),
        ))
    }
}

impl Peer {
    /// A stable label for a counter key. `unknown` rather than empty, so a deployment
    /// that forgets connect info still gets one shared bucket instead of an unbounded
    /// number of keys that all look the same.
    pub fn key(&self) -> String {
        match self.0 {
            Some(ip) => ip.to_string(),
            None => "unknown".to_string(),
        }
    }

    /// The address as it can be stored, or `None`. Deliberately not [`Peer::key`]: the
    /// audit column is `inet`, and "unknown" is not an address.
    pub fn ip(&self) -> Option<String> {
        self.0.map(|ip| ip.to_string())
    }
}

/// A fixed window: `limit` failures per `window_secs`, per key.
#[derive(Debug, Clone, Copy)]
pub struct Window {
    pub limit: i64,
    pub secs: u64,
}

/// Failed sign-ins, per source and account. Ten is enough for a person who has forgotten
/// which of their passwords this is, and not enough for a dictionary.
pub const LOGIN: Window = Window {
    limit: 10,
    secs: 60,
};

/// Account creation, per source. Higher: a household signing up several people shares one
/// address, and a support engineer creating demo accounts is not an attacker.
pub const REGISTER: Window = Window {
    limit: 20,
    secs: 60,
};

/// Claims, per source and serial. A device that fails to claim is usually misconfigured,
/// and it retries in a loop; that loop must not become a denial of service on the home.
pub const CLAIM: Window = Window {
    limit: 10,
    secs: 60,
};

/// Refuse if this key has already used its window. Does not count the attempt.
pub async fn check(state: &AppState, key: &str, window: Window) -> Result<(), ApiError> {
    match state.cache.peek(key).await {
        Ok(count) if i64::try_from(count).unwrap_or(i64::MAX) >= window.limit => {
            kernel::metrics::rate_limited();
            Err(ApiError(AppError::code(
                kernel::ErrorCode::RateLimited,
                "too many failed attempts; try again in a minute",
            )))
        }
        // A cache outage must not lock people out of their own homes: fail open on the
        // check and let the audit trail carry the information.
        _ => Ok(()),
    }
}

/// Count one failure. Best-effort: a limiter that cannot record is not a reason to return
/// an error to a caller whose request has already been answered.
pub async fn record(state: &AppState, key: &str, window: Window) {
    if let Err(e) = state
        .cache
        .incr_with_ttl(key, std::time::Duration::from_secs(window.secs))
        .await
    {
        tracing::warn!(error = ?e, "could not record a failed attempt");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peer_without_connect_info_still_gets_one_bucket() {
        assert_eq!(Peer(None).key(), "unknown");
        assert_eq!(
            Peer(Some("10.0.0.4".parse().expect("ip"))).key(),
            "10.0.0.4"
        );
    }
}
