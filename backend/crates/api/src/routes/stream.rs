//! `/v1/stream` — live state for every open app.
//!
//! The socket authenticates with the access token in its **first frame**, never
//! the query string (a URL lands in access logs and browser history), and
//! subscribes to the homes named in that same frame. Fan-out arrives over Redis,
//! so any API instance can serve any socket and the worker needs no socket to
//! every pod.

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::mpsc;
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
    let mut channels = Vec::new();
    for home_id in frame.homes {
        if state.homes.require(home_id, claims.sub).await.is_ok() {
            channels.push(format!("home:{home_id}"));
        }
    }

    if channels.is_empty() {
        let _ = socket
            .send(Message::Text(r#"{"t":"error","code":"no_homes"}"#.into()))
            .await;
        return;
    }

    let (tx, mut rx) = mpsc::channel::<String>(128);
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel::<()>();
    let url = state.cfg.redis_url.clone();
    let subscribed = channels.clone();
    tokio::spawn(async move {
        if let Err(e) = pump(url, subscribed, tx, ready_tx).await {
            tracing::warn!(error = ?e, "stream subscriber ended");
        }
    });

    // "ready" means subscribed, not merely authenticated: Redis pub/sub has no
    // replay, so a client that publishes right after "ready" must not be able to
    // beat the SUBSCRIBE.
    let _ = ready_rx.await;

    let _ = socket
        .send(Message::Text(
            serde_json::json!({ "t": "ready", "homes": channels })
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
            Some(event) = rx.recv() => {
                if socket.send(Message::Text(event.into())).await.is_err() { break; }
            }
        }
    }
}

/// One Redis subscription per socket. Redis pub/sub is cheap and this keeps the
/// code obvious; swap for a shared fan-out when a single pod holds thousands.
async fn pump(
    url: String,
    channels: Vec<String>,
    tx: mpsc::Sender<String>,
    ready: tokio::sync::oneshot::Sender<()>,
) -> anyhow::Result<()> {
    let client = redis::Client::open(url)?;
    let mut pubsub = client.get_async_pubsub().await?;
    for channel in &channels {
        pubsub.subscribe(channel).await?;
    }
    let _ = ready.send(());

    loop {
        let Some(message) = pubsub.on_message().next().await else {
            break;
        };
        let payload: String = message.get_payload()?;
        if tx.send(payload).await.is_err() {
            break;
        }
    }
    Ok(())
}
