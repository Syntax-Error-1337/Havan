//! `/v1/stream` — live state for every open app.
//!
//! The socket authenticates with the access token in its **first frame**, never
//! the query string (a URL lands in access logs and browser history), and
//! subscribes to the homes named in that same frame. Fan-out arrives over Redis,
//! so any API instance can serve any socket and the worker needs no socket to
//! every pod.
//!
//! One Redis subscription per *channel per process*, shared by every socket on it (`hub.rs`): a pod
//! with five thousand phones on one home holds one subscription, not five thousand.

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use futures_util::StreamExt;
use serde::Deserialize;
use uuid::Uuid;

use crate::app::AppState;

pub async fn ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| handle(socket, state))
}

#[derive(Deserialize)]
struct ClientFrame {
    #[serde(rename = "t")]
    kind: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    homes: Vec<Uuid>,
}

async fn handle(mut socket: WebSocket, state: AppState) {
    let Some(Ok(Message::Text(first))) = socket.recv().await else {
        return;
    };

    let Ok(frame) = serde_json::from_str::<ClientFrame>(&first) else {
        let _ = socket
            .send(Message::Text(r#"{"t":"error","code":"bad_frame"}"#.into()))
            .await;
        return;
    };
    if frame.kind != "auth" {
        let _ = socket
            .send(Message::Text(
                r#"{"t":"error","code":"auth_required"}"#.into(),
            ))
            .await;
        return;
    }

    let Some(token) = frame.token else {
        let _ = socket
            .send(Message::Text(
                r#"{"t":"error","code":"auth_required"}"#.into(),
            ))
            .await;
        return;
    };

    let Ok(claims) = state.identity.verify_access(&token) else {
        let _ = socket
            .send(Message::Text(
                r#"{"t":"error","code":"invalid_token"}"#.into(),
            ))
            .await;
        return;
    };

    // Membership is checked per home before a single event is forwarded.
    let mut homes = Vec::new();
    for home_id in frame.homes {
        if state.homes.require(home_id, claims.sub).await.is_ok() {
            homes.push(format!("home:{home_id}"));
        }
    }

    if homes.is_empty() {
        let _ = socket
            .send(Message::Text(r#"{"t":"error","code":"no_homes"}"#.into()))
            .await;
        return;
    }

    // Plus the person: an alert belongs to whoever has to read it, and one customer can be in several
    // homes. The token already proved who this socket belongs to, so no extra check applies here.
    let mut channels = homes.clone();
    channels.push(format!("user:{}", claims.sub));

    // One Redis subscription per channel per process; every socket on the channel listens to the same
    // in-process broadcast. `subscribe` only answers once Redis has acknowledged the SUBSCRIBE, so a
    // client that publishes right after "ready" still cannot beat it.
    let mut subscription = state.hub.subscribe(&channels).await;
    // Boxed so the closure's type can be named: `select_all` needs a concrete stream type, and each
    // receiver becomes an unfold over its own `broadcast::Receiver`.
    let streams: Vec<std::pin::Pin<Box<dyn futures_util::Stream<Item = String> + Send>>> =
        subscription
            .receivers_mut()
            .drain(..)
            .map(|rx| {
                Box::pin(futures_util::stream::unfold(rx, |mut rx| async move {
                    match rx.recv().await {
                        Ok(payload) => Some((payload, rx)),
                        Err(_) => None,
                    }
                }))
                    as std::pin::Pin<Box<dyn futures_util::Stream<Item = String> + Send>>
            })
            .collect();
    let mut events = futures_util::stream::select_all(streams);

    let _ = socket
        .send(Message::Text(
            serde_json::json!({ "t": "ready", "homes": homes })
                .to_string()
                .into(),
        ))
        .await;

    loop {
        tokio::select! {
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(Message::Ping(p))) => {
                    if socket.send(Message::Pong(p)).await.is_err() { break; }
                }
                // The v1 client has nothing to say after auth; ignoring unknown
                // frames keeps old clients working against a new server.
                Some(Ok(_)) => {}
                Some(Err(_)) => break,
            },
            Some(event) = events.next() => {
                if socket.send(Message::Text(event.into())).await.is_err() { break; }
            }
        }
    }

    // Handing the channels back is what stops the Redis task when this was the last socket on them.
    state.hub.release(subscription).await;
}
