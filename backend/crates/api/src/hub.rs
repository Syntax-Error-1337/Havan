//! One Redis subscription per channel per process — or one per pod, when a fan-out tier is running.
//!
//! The first version of `/v1/stream` opened a Redis pub/sub connection for every websocket. That is the
//! clearest possible code and it does not scale: a pod holding five thousand sockets held five thousand
//! subscriptions, and Redis did the fan-out one copy at a time. The comment in `stream.rs` said "swap for a
//! shared fan-out when a single pod holds thousands" — this is that swap.
//!
//! The shape is a refcounted channel:
//!
//! * the first socket to ask for `home:{id}` starts one task that subscribes once and forwards payloads into a
//!   `broadcast` channel;
//! * every other socket for that channel subscribes to the broadcast — in-process, no I/O;
//! * the last socket to leave stops the task and closes its Redis subscription.
//!
//! A device event therefore costs Redis one delivery per *pod*, not one per phone.
//!
//! ## The second mode: a pod with no subscriptions of its own
//!
//! One subscription per pod per channel is right until one home is served by many pods: ten pods on a busy
//! home hold ten subscriptions and Redis makes ten copies, which is the per-socket shape one order of magnitude
//! down. With `HAVEN_FANOUT=1` the pod stops subscribing to `home:*` entirely:
//!
//! * it writes its interest into Redis (`SADD haven.fanout.ch:{channel}`, 30-second expiry, refreshed by the
//!   forwarder's readers) and subscribes to exactly one channel of its own, `pod:{id}`;
//! * `haven-fanout` holds the single `PSUBSCRIBE home:* user:*` and republishes each event, wrapped in
//!   `{"ch":…,"p":…}`, to the pods that asked for that channel;
//! * the pod demultiplexes the envelope back onto the per-channel broadcasts its sockets already listen to.
//!
//! The envelope is why a socket cannot see another socket's events: one `pod:{id}` feed carries every channel
//! the pod serves, so each frame says which channel it belongs to and is routed only to that channel's
//! listeners.
//!
//! Failover is the forwarder's problem (`worker/src/bin/fanout.rs`): if it dies, pods keep writing interest
//! into a key nobody reads and their sockets go quiet, which the app's own reconnect and resync already
//! covers. Interest expires 30 seconds after a pod stops refreshing it, so a dead pod is not a black hole.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use futures_util::StreamExt;
use tokio::sync::{Mutex, broadcast, oneshot};

/// Deep enough that a slow socket cannot stall the others; a socket this far behind is not reading.
const FANOUT_BACKLOG: usize = 256;

/// How long a pod's interest in a channel survives without a refresh. The forwarder does not refresh it; the
/// pod does, every ten seconds, while it still holds a socket on that channel — so this is three missed
/// heartbeats, not one.
const INTEREST_TTL_SECS: u64 = 30;

struct Channel {
    /// How many sockets are listening. The task stops at zero.
    listeners: usize,
    tx: broadcast::Sender<String>,
    /// `None` in fan-out mode: the subscription belongs to the forwarder, not to this process.
    stop: Option<oneshot::Sender<()>>,
}

enum Mode {
    /// One Redis subscription per channel, held by this process.
    Direct,
    /// One subscription for the whole pod, demultiplexed by channel.
    Fanout { pod_id: String },
}

/// Refcounted fan-out for the websocket stream.
pub struct Hub {
    url: String,
    mode: Mode,
    channels: Mutex<HashMap<String, Channel>>,
    sockets: AtomicU64,
    /// The demux task, in fan-out mode: stopped when the last socket leaves.
    demux: Mutex<Option<oneshot::Sender<()>>>,
    /// One Redis connection for interest writes, created on first use. A connection per socket is what made
    /// 184 of a thousand sockets fail to connect inside a minute.
    interest: Mutex<Option<redis::aio::MultiplexedConnection>>,
}

/// What a socket holds while it is connected: one receiver per channel, and the promise to give them back.
pub struct Subscriptions {
    channel_names: Vec<String>,
    receivers: Vec<broadcast::Receiver<String>>,
}

impl Subscriptions {
    pub fn receivers_mut(&mut self) -> &mut Vec<broadcast::Receiver<String>> {
        &mut self.receivers
    }
}

impl Hub {
    pub fn new(url: impl Into<String>, fanout: bool) -> Arc<Self> {
        // A pod id is generated once per process. It has to be stable for as long as the process lives, because
        // it is what the forwarder addresses.
        let pod_id = uuid::Uuid::now_v7().to_string();
        Arc::new(Self {
            url: url.into(),
            mode: if fanout {
                Mode::Fanout { pod_id }
            } else {
                Mode::Direct
            },
            channels: Mutex::new(HashMap::new()),
            sockets: AtomicU64::new(0),
            demux: Mutex::new(None),
            interest: Mutex::new(None),
        })
    }

    /// Take a subscription to every channel named, starting whatever has to start.
    pub async fn subscribe(self: &Arc<Self>, channels: &[String]) -> Subscriptions {
        self.sockets.fetch_add(1, Ordering::Relaxed);

        if let Mode::Fanout { pod_id } = &self.mode {
            let pod_id = pod_id.clone();
            self.ensure_demux(&pod_id).await;
            // Interest before "ready": the forwarder must know about this channel before the client is told it
            // can publish, or the first event after connecting is delivered to nobody.
            self.write_interest(channels, true).await;
        }

        let mut guard = self.channels.lock().await;
        let mut receivers = Vec::with_capacity(channels.len());
        // New channels are not ready when the task is spawned, only when Redis acknowledges the SUBSCRIBE. The
        // caller must not send "ready" before that: pub/sub has no replay, and an event published in the gap is
        // lost.
        let mut starting = Vec::new();

        for channel in channels {
            if let Some(existing) = guard.get_mut(channel) {
                existing.listeners += 1;
                receivers.push(existing.tx.subscribe());
                continue;
            }

            let (tx, _) = broadcast::channel(FANOUT_BACKLOG);
            receivers.push(tx.subscribe());

            let stop = match self.mode {
                Mode::Fanout { .. } => {
                    // No task per channel: the pod feed already carries this channel's frames.
                    None
                }
                Mode::Direct => {
                    let (stop_tx, stop_rx) = oneshot::channel();
                    let (ready_tx, ready_rx) = oneshot::channel();
                    starting.push(ready_rx);

                    let url = self.url.clone();
                    let name = channel.clone();
                    let forward = tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = pump(&url, &name, forward, stop_rx, ready_tx).await {
                            // A subscription that dies leaves its sockets silent; the client's own reconnect
                            // logic is what recovers, so this is a warning and not a fatal.
                            tracing::warn!(channel = %name, error = ?e, "stream channel ended");
                        }
                    });
                    Some(stop_tx)
                }
            };

            guard.insert(
                channel.clone(),
                Channel {
                    listeners: 1,
                    tx,
                    stop,
                },
            );
        }

        drop(guard);

        // Wait for every channel that had to be started, not for the ones already running.
        for ready in starting {
            let _ = ready.await;
        }

        Subscriptions {
            channel_names: channels.to_vec(),
            receivers,
        }
    }

    /// Give the subscriptions back: the last socket on a channel stops its task.
    pub async fn release(&self, subscription: Subscriptions) {
        self.sockets.fetch_sub(1, Ordering::Relaxed);

        let mut guard = self.channels.lock().await;
        let mut empty = Vec::new();
        for channel in subscription.channel_names {
            let Some(entry) = guard.get_mut(&channel) else {
                continue;
            };
            entry.listeners = entry.listeners.saturating_sub(1);
            if entry.listeners == 0 {
                // Taken out of the map, so the stop signal actually lands on the owning task.
                if let Some(closing) = guard.remove(&channel)
                    && let Some(stop) = closing.stop
                {
                    let _ = stop.send(());
                }
                empty.push(channel);
            }
        }
        drop(guard);

        if let Mode::Fanout { .. } = &self.mode
            && !empty.is_empty()
        {
            self.write_interest(&empty, false).await;
        }

        // The last socket on this pod takes the demux task with it.
        if self.sockets.load(Ordering::Relaxed) == 0 {
            let mut demux = self.demux.lock().await;
            if let Some(stop) = demux.take() {
                let _ = stop.send(());
            }
        }
    }

    /// Channels with at least one local listener. Exported so an operator can see the fan-out working: this
    /// number should track *homes*, not phones.
    pub async fn channel_count(&self) -> usize {
        self.channels.lock().await.len()
    }

    pub fn socket_count(&self) -> u64 {
        self.sockets.load(Ordering::Relaxed)
    }

    /// Is this pod using the shared fan-out tier? Exported on `/metrics` so a scrape can tell the two modes
    /// apart at a glance — the channel gauge means something different in each.
    pub fn is_fanout(&self) -> bool {
        matches!(self.mode, Mode::Fanout { .. })
    }

    /// `SADD`/`SREM` for each channel, plus the TTL. Failure is logged, not fatal: a pod that cannot write
    /// interest keeps serving its existing sockets and the next heartbeat tries again.
    ///
    /// One connection is created and reused. The first version opened a new Redis connection per socket, which
    /// measured as 816 of 1 000 sockets able to connect inside a minute — the fan-out worked and the connection
    /// storm around it did not.
    async fn write_interest(&self, channels: &[String], add: bool) {
        let Mode::Fanout { pod_id } = &self.mode else {
            return;
        };

        let mut guard = self.interest.lock().await;
        if guard.is_none() {
            let Ok(client) = redis::Client::open(self.url.clone()) else {
                tracing::warn!("fan-out: redis client for interest failed");
                return;
            };
            match client.get_multiplexed_async_connection().await {
                Ok(conn) => *guard = Some(conn),
                Err(e) => {
                    tracing::warn!(error = ?e, "fan-out: no connection for interest");
                    return;
                }
            }
        }
        let Some(conn) = guard.as_mut() else {
            return;
        };

        for channel in channels {
            let key = format!("haven.fanout.ch:{channel}");
            let result = if add {
                let _: Result<i64, _> = redis::cmd("SADD")
                    .arg(&key)
                    .arg(pod_id)
                    .query_async(conn)
                    .await;
                redis::cmd("EXPIRE")
                    .arg(&key)
                    .arg(INTEREST_TTL_SECS)
                    .query_async::<i64>(conn)
                    .await
            } else {
                let _: Result<i64, _> = redis::cmd("SREM")
                    .arg(&key)
                    .arg(pod_id)
                    .query_async(conn)
                    .await;
                Ok(0)
            };
            if let Err(e) = result {
                tracing::warn!(channel = %channel, error = ?e, "fan-out: interest write failed");
            }
        }
    }

    /// Start the single `pod:{id}` subscription that feeds every channel this pod serves.
    async fn ensure_demux(self: &Arc<Self>, pod_id: &str) {
        let mut guard = self.demux.lock().await;
        if guard.is_some() {
            return;
        }

        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let weak: Weak<Hub> = Arc::downgrade(self);
        let url = self.url.clone();
        let channel = format!("pod:{pod_id}");
        let ready = tokio::sync::oneshot::channel::<()>();
        let ready_tx = ready.0;
        let ready_rx = ready.1;

        tokio::spawn(async move {
            if let Err(e) = demux(&url, &channel, weak, stop_rx, ready_tx).await {
                tracing::warn!(channel = %channel, error = ?e, "fan-out demux ended");
            }
        });

        // Subscribed before any socket is told "ready", same rule as the direct mode.
        let _ = ready_rx.await;
        *guard = Some(stop_tx);
    }
}

/// One Redis subscription, forwarding to every local listener of that channel.
async fn pump(
    url: &str,
    channel: &str,
    tx: broadcast::Sender<String>,
    mut stop: oneshot::Receiver<()>,
    ready: oneshot::Sender<()>,
) -> anyhow::Result<()> {
    let client = redis::Client::open(url)?;
    let mut pubsub = client.get_async_pubsub().await?;
    pubsub.subscribe(channel).await?;
    // Subscribed, so the socket may now be told "ready".
    let _ = ready.send(());

    let mut messages = pubsub.on_message();
    loop {
        tokio::select! {
            // Dropping the sender side of `stop` also means "nobody is listening any more".
            _ = &mut stop => break,
            message = messages.next() => {
                let Some(message) = message else { break };
                let payload: String = message.get_payload()?;
                // `send` fails when every receiver has gone; that is not an error, it is the last socket
                // leaving before `release` runs.
                let _ = tx.send(payload);
            }
        }
    }
    // Leaving the loop drops the connection, and Redis drops the subscription with it. An explicit
    // unsubscribe would need the borrow `messages` still holds.
    Ok(())
}

/// The pod's single subscription: `{"ch":…,"p":…}` in, per-channel broadcast out.
async fn demux(
    url: &str,
    channel: &str,
    hub: Weak<Hub>,
    mut stop: oneshot::Receiver<()>,
    ready: oneshot::Sender<()>,
) -> anyhow::Result<()> {
    let client = redis::Client::open(url)?;
    let mut pubsub = client.get_async_pubsub().await?;
    pubsub.subscribe(channel).await?;
    let _ = ready.send(());

    let mut messages = pubsub.on_message();
    // Interest is written once when a socket arrives, and it expires: without this the pod would go quiet thirty
    // seconds after its last new socket. Refreshing is a `SADD` (idempotent) plus an `EXPIRE` on the channels the
    // pod still serves.
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(10));
    loop {
        tokio::select! {
            _ = &mut stop => break,
            _ = heartbeat.tick() => {
                let Some(hub) = hub.upgrade() else { break };
                let names: Vec<String> = { hub.channels.lock().await.keys().cloned().collect() };
                if !names.is_empty() {
                    hub.write_interest(&names, true).await;
                }
            }
            message = messages.next() => {
                let Some(message) = message else { break };
                let payload: String = message.get_payload()?;

                // The envelope says which channel this belongs to. A frame that does not parse is dropped with a
                // warning rather than killing the feed for every socket on the pod.
                let Ok(envelope) = serde_json::from_str::<serde_json::Value>(&payload) else {
                    tracing::warn!("fan-out: unreadable envelope");
                    continue;
                };
                let (Some(target), Some(body)) = (
                    envelope.get("ch").and_then(|v| v.as_str()),
                    envelope.get("p").and_then(|v| v.as_str()),
                ) else {
                    tracing::warn!("fan-out: envelope without channel or payload");
                    continue;
                };

                let Some(hub) = hub.upgrade() else { break };
                let guard = hub.channels.lock().await;
                if let Some(entry) = guard.get(target) {
                    let _ = entry.tx.send(body.to_string());
                }
            }
        }
    }
    Ok(())
}
