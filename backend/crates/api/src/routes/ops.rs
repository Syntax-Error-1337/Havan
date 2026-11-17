//! Operator endpoints: the ones a person with a shell uses, not a customer.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::app::AppState;
use crate::error::ApiError;
use crate::limits::Peer;

/// The operator's door.
///
/// Loopback is allowed without a token because that is how a sidecar or a local scrape
/// reaches it, and requiring a secret there buys nothing. Anywhere else needs
/// `x-ops-token` matching `OPS_TOKEN`; with no `OPS_TOKEN` configured the endpoint is
/// unreachable from outside the machine rather than open.
pub fn require_ops(state: &AppState, headers: &HeaderMap, peer: &Peer) -> Result<(), ApiError> {
    if peer.0.is_some_and(|ip| ip.is_loopback()) {
        return Ok(());
    }

    let presented = headers
        .get("x-ops-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let expected = state.cfg.ops_token.as_ref().map(|s| s.expose());

    match expected {
        Some(expected) if constant_time_eq(presented, expected) => Ok(()),
        _ => Err(ApiError(kernel::AppError::code(
            kernel::ErrorCode::Unauthorized,
            "this endpoint needs a valid x-ops-token",
        ))),
    }
}

/// `GET /metrics` — Prometheus text.
pub async fn metrics(State(state): State<AppState>, headers: HeaderMap, peer: Peer) -> Response {
    if let Err(refusal) = require_ops(&state, &headers, &peer) {
        return refusal.into_response();
    }

    // The theme is the one read on every cold start of every install, so its cache is worth a
    // number: if this stays at zero the cache is not working, and if it climbs the database is not
    // being asked for an answer that has not changed.
    let theme_hits = state.theme.cache_hits();

    // The stream's fan-out, in two numbers the operator can compare: channels should track *homes with
    // a phone open*, sockets should track those phones. When the first version of this endpoint shipped,
    // channels and sockets were the same number, because every socket held its own subscription.
    let channels = state.hub.channel_count().await;
    let sockets = state.hub.socket_count();

    let body = format!(
        "{}# HELP haven_theme_cache_hits_total Theme reads answered without a database query.\n\
         # TYPE haven_theme_cache_hits_total counter\n\
         haven_theme_cache_hits_total {theme_hits}\n\
         # HELP haven_stream_channels Redis subscriptions held by this process.\n\
         # TYPE haven_stream_channels gauge\n\
         haven_stream_channels {channels}\n\
         # HELP haven_stream_sockets Open websockets on this process.\n\
         # TYPE haven_stream_sockets gauge\n\
         haven_stream_sockets {sockets}\n",
        kernel::metrics::render()
    );

    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        body,
    )
        .into_response()
}

/// Compare without an early return, so a wrong token does not leak its first correct
/// character through timing. Length is compared first because that is not a secret.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_compare_correctly_and_never_short_circuit_on_content() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
        assert!(!constant_time_eq("", "abc"));
    }
}
