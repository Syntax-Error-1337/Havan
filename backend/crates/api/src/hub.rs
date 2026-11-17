//! One Redis subscription per channel per process, not one per socket.
//!
//! The first version of `/v1/stream` opened a Redis pub/sub connection for every websocket. That is the
//! clearest possible code and it does not scale: a pod holding five thousand sockets held five thousand
//! subscriptions, and Redis does the fan-out for us one copy at a time. The comment in `stream.rs` said
//! "swap for a shared fan-out when a single pod holds thousands" — this is that swap.
//!
//! The shape is a refcounted channel:
//!
//! * the first socket to ask for `home:{id}` starts one task that subscribes once and forwards payloads
//!   into a `broadcast` channel;
//! * every other socket for that channel subscribes to the broadcast — in-process, no I/O;
//! * the last socket to leave stops the task and closes its Redis subscription.
//!
//! A device event now costs Redis one delivery per *pod*, not one per phone.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use futures_util::StreamExt;
use tokio::sync::{Mutex, broadcast, oneshot};

/// Deep enough that a slow socket cannot stall the others; a socket this far behind is not reading.
const FANOUT_BACKLOG: usize = 256;

struct Channel {
    /// How many sockets are listening. The task stops at zero.
    listeners: usize,
    tx: broadcast::Sender<String>,
    stop: oneshot::Sender<()>,
}

/// Refcounted Redis fan-out for the websocket stream.
pub struct Hub {
    url: String,
    channels: Mutex<HashMap<String, Channel>>,
    sockets: AtomicU64,
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
    pub fn new(url: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            url: url.into(),
            channels: Mutex::new(HashMap::new()),
            sockets: AtomicU64::new(0),
        })
    }

    /// Take a subscription to every channel named, starting whatever has to start.
    pub async fn subscribe(self: &Arc<Self>, channels: &[String]) -> Subscriptions {
        self.sockets.fetch_add(1, Ordering::Relaxed);

        let mut guard = self.channels.lock().await;
        let mut receivers = Vec::with_capacity(channels.len());
        // New channels are not ready when the task is spawned, only when Redis acknowledges the
        // SUBSCRIBE. The caller must not send "ready" to the client before that: pub/sub has no replay,
        // and an event published in the gap is lost.
        let mut starting = Vec::new();

        for channel in channels {
            if let Some(existing) = guard.get_mut(channel) {
                existing.listeners += 1;
                receivers.push(existing.tx.subscribe());
                continue;
            }

            let (tx, _) = broadcast::channel(FANOUT_BACKLOG);
            let (stop_tx, stop_rx) = oneshot::channel();
            let (ready_tx, ready_rx) = oneshot::channel();
            starting.push(ready_rx);
            receivers.push(tx.subscribe());

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

            guard.insert(
                channel.clone(),
                Channel {
                    listeners: 1,
                    tx,
                    stop: stop_tx,
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

    /// Give the subscriptions back: the last socket on a channel stops its Redis task.
    pub async fn release(&self, subscription: Subscriptions) {
        self.sockets.fetch_sub(1, Ordering::Relaxed);

        let mut guard = self.channels.lock().await;
        for channel in subscription.channel_names {
            let Some(entry) = guard.get_mut(&channel) else {
                continue;
            };
            entry.listeners = entry.listeners.saturating_sub(1);
            if entry.listeners == 0 {
                // Taken out of the map, so the stop signal actually lands on the owning task.
                if let Some(closing) = guard.remove(&channel) {
                    let _ = closing.stop.send(());
                }
            }
        }
    }

    /// Channels with at least one local listener. Exported so an operator can see the fan-out working:
    /// this number should track *homes*, not phones.
    pub async fn channel_count(&self) -> usize {
        self.channels.lock().await.len()
    }

    pub fn socket_count(&self) -> u64 {
        self.sockets.load(Ordering::Relaxed)
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
                // `send` fails when every receiver has gone; that is not an error, it is the last
                // socket leaving before `release` runs.
                let _ = tx.send(payload);
            }
        }
    }
    // Leaving the loop drops the connection, and Redis drops the subscription with it. An explicit
    // unsubscribe would need the borrow `messages` still holds.
    Ok(())
}
