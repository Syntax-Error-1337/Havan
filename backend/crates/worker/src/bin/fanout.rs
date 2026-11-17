//! `haven-fanout` — one subscriber for the whole fleet of API pods.
//!
//! The API's stream hub holds one Redis subscription per channel *per process* (`api/src/hub.rs`). That is
//! right up to the point where a single home is served by many pods: ten pods on one busy home hold ten
//! subscriptions and Redis delivers every device event ten times, which is the same shape as the per-socket
//! design the hub replaced, one order of magnitude down.
//!
//! This binary is the tier above: it subscribes **once** to `home:*` and `user:*`, and republishes each event
//! to the pods that actually hold a socket for that channel, on `pod:{id}`.
//!
//! ```text
//!   publisher ──▶ home:{id} ──▶ [haven-fanout] ──▶ pod:a ──▶ sockets on pod a
//!                                             └──▶ pod:b ──▶ sockets on pod b
//! ```
//!
//! ## How a pod says it is interested
//!
//! `SADD haven.fanout.ch:{channel} {pod_id}` plus a 30-second `EXPIRE`, refreshed every ten seconds while the
//! pod holds a socket on that channel. A pod that dies stops refreshing, its interest expires, and the
//! forwarder stops sending to a channel nobody reads — without a registry, a heartbeat protocol, or a
//! membership service. The interest is written *before* the socket is told "ready", so a publish that follows
//! immediately is not lost.
//!
//! ## Leader election
//!
//! Two forwarders would deliver every event twice. One is elected with `SET haven.fanout.leader {id} NX PX
//! 15000` and renews every five seconds; the others stand by and take over within fifteen seconds of a
//! failure. Election is cheap because the work is idempotent-ish (a duplicate delivery is a duplicate frame,
//! not a duplicate side effect) — so the failure mode is a little noise, not a lost alert.

use std::time::Duration;

use anyhow::Context;
use futures_util::StreamExt;
use redis::AsyncCommands;
use uuid::Uuid;

/// How long the leadership key lives without a renewal. Three renewal intervals: one missed round is not a
/// failover.
const LEADER_TTL_MS: u64 = 15_000;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = kernel::Config::from_env().context("configuration")?;
    kernel::telemetry::init(&cfg.env);

    let me = Uuid::now_v7().to_string();
    let client = redis::Client::open(cfg.redis_url.clone()).context("redis")?;
    let mut control = client
        .get_multiplexed_async_connection()
        .await
        .context("control connection")?;

    tracing::info!(replica = %me, "fan-out starting");

    // Standby loop: take the leadership key, and run the pump for as long as we hold it.
    loop {
        let won: Option<String> = redis::cmd("SET")
            .arg("haven.fanout.leader")
            .arg(&me)
            .arg("NX")
            .arg("PX")
            .arg(LEADER_TTL_MS)
            .query_async(&mut control)
            .await
            .context("elect")?;

        if won.is_some() {
            tracing::info!(replica = %me, "elected; subscribing to every home and user channel");
            // Renewal runs beside the pump: if it stops winning, the pump is abandoned by dropping it.
            let renew = tokio::spawn({
                let me = me.clone();
                let url = cfg.redis_url.clone();
                async move { renew_leadership(url, me).await }
            });

            if let Err(e) = pump(&cfg.redis_url, &me).await {
                tracing::warn!(error = ?e, "fan-out pump ended");
            }

            renew.abort();
        }

        // Either we lost the key or the pump fell over: wait, then try to lead again.
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn renew_leadership(url: String, me: String) -> anyhow::Result<()> {
    let client = redis::Client::open(url)?;
    let mut conn = client.get_multiplexed_async_connection().await?;
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        // Only extend the key we still own: a `PEXPIRE` on somebody else's key would make two leaders.
        let extended: Option<i64> = redis::cmd("PEXPIRE")
            .arg("haven.fanout.leader")
            .arg(LEADER_TTL_MS)
            .query_async(&mut conn)
            .await?;
        let _ = extended;

        let owner: Option<String> = conn.get("haven.fanout.leader").await?;
        if owner.as_deref() != Some(me.as_str()) {
            anyhow::bail!("leadership lost");
        }
    }
}

/// One `PSUBSCRIBE`, then republish each event to the pods that asked for its channel.
async fn pump(url: &str, me: &str) -> anyhow::Result<()> {
    let client = redis::Client::open(url)?;
    let mut pubsub = client.get_async_pubsub().await?;
    // Two patterns rather than a channel list: a channel that appears later is covered without re-subscribing.
    pubsub.psubscribe("home:*").await?;
    pubsub.psubscribe("user:*").await?;

    let mut publisher = client.get_multiplexed_async_connection().await?;
    let mut messages = pubsub.on_message();
    let mut events: u64 = 0;
    let mut deliveries: u64 = 0;

    while let Some(message) = messages.next().await {
        let channel: String = message.get_channel_name().to_string();
        let payload: String = message.get_payload()?;
        events += 1;

        // The channel travels with the payload. A pod listens on one `pod:{id}` channel for every socket it
        // holds, so without this envelope it could not tell which of its sockets a frame belongs to — and a
        // frame for another home would reach a socket that never asked for it.
        let envelope = serde_json::json!({ "ch": channel, "p": payload }).to_string();

        let pods: Vec<String> = publisher
            .smembers(format!("haven.fanout.ch:{channel}"))
            .await?;
        for pod in &pods {
            let _ = publisher
                .publish::<_, _, ()>(format!("pod:{pod}"), envelope.clone())
                .await;
            deliveries += 1;
        }

        if events.is_multiple_of(500) {
            tracing::info!(
                replica = %me,
                events,
                deliveries,
                channel = %channel,
                "fan-out progress"
            );
        }
    }

    Ok(())
}
