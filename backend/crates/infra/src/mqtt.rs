//! MQTT client for the cloud side: command down, events up, shadow on connect.
//!
//! The broker is VerneMQ and it is not ours to reimplement. This module owns the
//! two things that are ours: the topic strings (from `protocol::topics`) and the
//! decision to publish QoS 1 with a bounded in-flight window.

use kernel::AppError;
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tracing::{info, warn};

/// Client id used by every cloud process. `worker` appends its instance index so
/// two replicas do not fight over one session.
pub const KEEP_ALIVE: Duration = Duration::from_secs(30);
pub const INFLIGHT: u16 = 16;

#[derive(Clone)]
pub struct Mqtt {
    client: AsyncClient,
    /// Set by the event loop, read by `/healthz`. A handle that exists is not a
    /// connection: the client will happily queue a publish into a dead socket, so
    /// the only honest signal is the last event the loop actually saw.
    connected: Arc<AtomicBool>,
}

impl Mqtt {
    /// Connect and spawn the event loop. The event loop is what actually moves
    /// bytes; the client is just a handle, which is why it is cloned freely.
    pub fn connect(url: &str, client_id: &str) -> Result<Self, AppError> {
        let (host, port) = parse_host_port(url)?;
        let mut opts = MqttOptions::new(client_id, host, port);
        opts.set_keep_alive(KEEP_ALIVE);
        opts.set_inflight(INFLIGHT);
        // Clean session on purpose: this client publishes commands and subscribes to nothing
        // (app fan-out is Redis pub/sub). A persistent session would buy nothing and leak one
        // per replica restart, since the client id is unique per process.
        opts.set_clean_session(true);

        let (client, eventloop) = AsyncClient::new(opts, usize::from(INFLIGHT) * 4);
        let connected = Arc::new(AtomicBool::new(false));
        Self::spawn(eventloop, connected.clone());
        Ok(Self { client, connected })
    }

    fn spawn(mut eventloop: EventLoop, connected: Arc<AtomicBool>) {
        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        connected.store(true, Ordering::Relaxed);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        // A poll error is the loop's only evidence of a dead link.
                        connected.store(false, Ordering::Relaxed);
                        warn!(error = %e, "mqtt event loop error; backing off");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        });
    }

    /// True once the broker has acknowledged a connection and no poll has failed
    /// since.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// Publish, or say why not.
    ///
    /// `rumqttc::publish` only hands the packet to the client's queue: with the broker
    /// down it accepts the packet and returns `Ok`, and the message is dropped when the
    /// event loop notices. That made a command look sent while nothing left the process.
    /// Checking the connection first is the difference between "queued on the wire" and
    /// "believed".
    pub async fn publish(&self, topic: &str, payload: Vec<u8>) -> Result<(), AppError> {
        if !self.is_connected() {
            return Err(AppError::Broker(Box::new(std::io::Error::other(
                "broker is not connected",
            ))));
        }
        self.client
            .publish(topic, QoS::AtLeastOnce, false, payload)
            .await
            .map_err(|e| AppError::Broker(Box::new(e)))
    }

    pub async fn publish_retained(&self, topic: &str, payload: Vec<u8>) -> Result<(), AppError> {
        self.client
            .publish(topic, QoS::AtLeastOnce, true, payload)
            .await
            .map_err(|e| AppError::Broker(Box::new(e)))
    }

    pub async fn subscribe(&self, filter: &str) -> Result<(), AppError> {
        info!(filter, "subscribing");
        self.client
            .subscribe(filter, QoS::AtLeastOnce)
            .await
            .map_err(|e| AppError::Broker(Box::new(e)))
    }
}

/// `mqtt://host:1883` or `mqtts://host:8883` → `(host, port)`.
/// TLS is configured by the deployment; this only splits the address.
fn parse_host_port(url: &str) -> Result<(String, u16), AppError> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| AppError::Internal(format!("bad MQTT_URL: {url}")))?;
    let default_port = match scheme {
        "mqtt" => 1883,
        "mqtts" | "ssl" => 8883,
        other => {
            return Err(AppError::Internal(format!(
                "unsupported MQTT scheme {other}"
            )));
        }
    };
    let rest = rest.trim_end_matches('/');
    match rest.rsplit_once(':') {
        Some((host, port)) => Ok((
            host.to_string(),
            port.parse()
                .map_err(|_| AppError::Internal(format!("bad MQTT port in {url}")))?,
        )),
        None => Ok((rest.to_string(), default_port)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_urls() {
        assert_eq!(
            parse_host_port("mqtt://localhost:1883").unwrap(),
            ("localhost".to_string(), 1883)
        );
        assert_eq!(
            parse_host_port("mqtts://mqtt.haven.dev").unwrap(),
            ("mqtt.haven.dev".to_string(), 8883)
        );
        assert_eq!(
            parse_host_port("mqtts://h:9999/").unwrap(),
            ("h".to_string(), 9999)
        );
        assert!(parse_host_port("http://h:80").is_err());
    }
}
