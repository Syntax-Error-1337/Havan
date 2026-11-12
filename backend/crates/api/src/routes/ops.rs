//! Operator endpoints: the ones a person with a shell uses, not a customer.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::app::AppState;
use crate::limits::Peer;

/// `GET /metrics` — Prometheus text.
///
/// Loopback is allowed without a token because that is how a sidecar or a local scrape
/// reaches it, and requiring a secret there buys nothing. Anywhere else needs
/// `x-ops-token` matching `OPS_TOKEN`; with no `OPS_TOKEN` configured the endpoint is
/// unreachable from outside the machine rather than open.
pub async fn metrics(State(state): State<AppState>, headers: HeaderMap, peer: Peer) -> Response {
    let loopback = peer.0.is_some_and(|ip| ip.is_loopback());

    if !loopback {
        let presented = headers
            .get("x-ops-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let expected = state.cfg.ops_token.as_ref().map(|s| s.expose());

        match expected {
            Some(expected) if constant_time_eq(presented, expected) => {}
            _ => {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": { "code": "unauthorized", "message": "metrics need a valid x-ops-token" }
                    })),
                )
                    .into_response();
            }
        }
    }

    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        kernel::metrics::render(),
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
