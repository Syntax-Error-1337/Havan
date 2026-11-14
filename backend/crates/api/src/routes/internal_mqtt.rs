//! The broker's questions, answered by the platform.
//!
//! VerneMQ's `vmq_webhooks` plugin POSTs every `auth_on_register`, `auth_on_publish` and
//! `auth_on_subscribe` decision to an HTTP endpoint and does what the endpoint says. That makes
//! the platform the authority on *who* a device is and *what* it may touch, instead of
//! duplicating device rows into a broker password file that drifts the moment a device is claimed.
//!
//! ## Why this exists
//!
//! The 8883 listener identifies a device by its client certificate, which is the strongest
//! credential we can issue and needs no server-side state at connect time. The 8884 listener
//! exists for devices that cannot afford a mutual handshake at all (an ESP8266 runs out of heap
//! partway through one), and on that listener a device logs in with `username = its device id`
//! and `password = its secret`. A password alone would be a weak credential: this file is what
//! stops it being one, because the topics a device may publish to or read are derived from the
//! same row that authenticated it, never from anything the device asked for.
//!
//! ## The shared secret
//!
//! These routes are not user-facing and carry no bearer token; they are called by the broker,
//! which authenticates with `HAVEN_MQTT_HOOK_TOKEN` in a header. Without that check the endpoint
//! would be an oracle: anyone who could reach the API could ask "is this device secret correct?"
//! as fast as they liked.

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

use crate::app::AppState;
use crate::error::ApiResult;

/// The platform's own identity on the broker. Fixed rather than configurable: the password is the
/// secret, and a name that never varies is one less thing to get wrong in four services' configs.
pub const SERVICE_USER: &str = "haven-service";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/internal/mqtt/register", post(on_register))
        .route("/internal/mqtt/authorize", post(on_authorize))
}

/// What VerneMQ sends for `auth_on_register`.
#[derive(Deserialize)]
struct RegisterBody {
    username: Option<String>,
    password: Option<String>,
}

/// What VerneMQ sends for `auth_on_publish` and `auth_on_subscribe`, which share a shape: a
/// topic and the session it came from. The hook name is in the `vernemq-hook` header.
#[derive(Deserialize)]
struct AuthorizeBody {
    username: Option<String>,
    topic: Option<String>,
    topics: Option<Vec<TopicRequest>>,
}

#[derive(Deserialize)]
struct TopicRequest {
    topic: String,
}

/// Constant-time comparison for the broker's token.
///
/// A `==` on a shared secret leaks its prefix through timing. This is not a defence against
/// someone with local access — it is the difference between a check and a gesture.
fn token_matches(headers: &HeaderMap, query: &HashMap<String, String>, expected: &str) -> bool {
    // A query parameter, because `vmq_webhooks` cannot send custom headers: the schema offers a
    // hook, an endpoint and a timeout, nothing else. The header is still accepted so the endpoint
    // can be called by something that *can* set one.
    let given = query
        .get("token")
        .map(String::as_str)
        .or_else(|| headers.get("x-haven-broker").and_then(|v| v.to_str().ok()));
    let Some(given) = given else {
        return false;
    };
    if given.len() != expected.len() {
        return false;
    }
    given
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

/// `auth_on_register`: is this device id and secret a real, claimed device?
async fn on_register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Json(body): Json<RegisterBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    if !token_matches(&headers, &query, &state.cfg.mqtt_hook_token) {
        return Ok((
            StatusCode::OK,
            Json(json!({"result": {"error": "not_allowed"}})),
        ));
    }

    let (Some(username), Some(password)) = (body.username, body.password) else {
        // `next`, not `not_allowed`: this plugin is installed on every listener, so a flat denial
        // here would also take out the workers and the simulator, which connect anonymously on
        // 1883. `next` falls through to the listener's own policy, which compose sets to
        // `allow_anonymous = on` — a dev stand-in, and the reason 1884/8883 are the only
        // device-facing ports. Removing anonymous access entirely means giving the workers and
        // the simulator credentials too; that is the next step, not something to fake here.
        return Ok((StatusCode::OK, Json(json!({"result": "next"}))));
    };

    // The platform's own clients — the API that publishes commands, the worker that ingests, the
    // simulator — are not devices and have no device row. They authenticate as one service
    // identity, and only that identity is allowed outside a device's own topic prefix.
    if username == SERVICE_USER {
        return if password == state.cfg.mqtt_service_token {
            Ok((StatusCode::OK, Json(json!({"result": "ok"}))))
        } else {
            tracing::warn!("broker presented the service identity with the wrong password");
            Ok((
                StatusCode::OK,
                Json(json!({"result": {"error": "not_allowed"}})),
            ))
        };
    }

    let Ok(device_id) = uuid::Uuid::parse_str(&username) else {
        return Ok((
            StatusCode::OK,
            Json(json!({"result": {"error": "not_allowed"}})),
        ));
    };

    match state
        .pairing
        .authenticate_device(device_id, &password)
        .await
    {
        Ok(device) => {
            tracing::info!(device = %device.device_id, "broker authenticated a device with its secret");
            // `max_message_size` is not a policy we set here; the envelope limits are enforced
            // by the API and the SDK. Returning `ok` alone keeps the broker's defaults.
            Ok((StatusCode::OK, Json(json!({"result": "ok"}))))
        }
        Err(e) => {
            // The reason is logged, never returned: VerneMQ's own log is the place for detail, and
            // a device must not learn whether the id exists or the secret is wrong.
            tracing::warn!(device = %username, error = %e, "broker refused a device credential");
            Ok((
                StatusCode::OK,
                Json(json!({"result": {"error": "not_allowed"}})),
            ))
        }
    }
}

/// `auth_on_publish` / `auth_on_subscribe`: may this device touch this topic?
async fn on_authorize(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
    Json(body): Json<AuthorizeBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    if !token_matches(&headers, &query, &state.cfg.mqtt_hook_token) {
        return Ok((
            StatusCode::OK,
            Json(json!({"result": {"error": "not_allowed"}})),
        ));
    }

    let Some(username) = body.username else {
        // Same reason as `on_register`: the workers' own traffic arrives without a username.
        return Ok((StatusCode::OK, Json(json!({"result": "next"}))));
    };
    // The platform publishes commands and subscribes to every device's traffic; it is the one
    // identity that is allowed to, because it is the thing doing the routing.
    if username == SERVICE_USER {
        return Ok((StatusCode::OK, Json(json!({"result": "ok"}))));
    }

    let Ok(device_id) = uuid::Uuid::parse_str(&username) else {
        return Ok((
            StatusCode::OK,
            Json(json!({"result": {"error": "not_allowed"}})),
        ));
    };

    // The device is re-read here rather than trusted from the session: a device removed from its
    // home must lose its topics immediately, not when its connection happens to drop.
    let device = match state.pairing.session_for(device_id).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(device = %device_id, error = %e, "broker refused a topic request");
            return Ok((
                StatusCode::OK,
                Json(json!({"result": {"error": "not_allowed"}})),
            ));
        }
    };

    let topics: Vec<&str> = match (&body.topic, &body.topics) {
        (Some(topic), _) => vec![topic.as_str()],
        (None, Some(list)) => list.iter().map(|t| t.topic.as_str()).collect(),
        (None, None) => Vec::new(),
    };

    // All or nothing for a subscribe batch: a subscription that is half allowed is a client that
    // believes it is listening to something it is not.
    let allowed = !topics.is_empty()
        && topics
            .iter()
            .all(|topic| domain::mqtt_acl::may_use_topic(&device, topic));

    if !allowed {
        tracing::warn!(device = %device_id, topics = ?topics, "broker refused a topic outside the device's own");
        return Ok((
            StatusCode::OK,
            Json(json!({"result": {"error": "not_allowed"}})),
        ));
    }

    Ok((StatusCode::OK, Json(json!({"result": "ok"}))))
}
