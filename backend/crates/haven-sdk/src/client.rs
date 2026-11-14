//! The connection, and the two things a device must never get wrong: which topics, and
//! what happens after a reconnect.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{Context, Result};
use protocol::envelope::{AckMsg, CommandMsg, DpValue, EventMsg, OtaReport, ShadowMsg};
use protocol::topics;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use uuid::Uuid;

use crate::{OtaOffer, PROTOCOL_VERSION, StateReport};

/// Where this device lives in the namespace. Built once, from the claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub vendor: String,
    pub home: String,
    pub device: String,
}

impl Link {
    pub fn new(
        vendor: impl Into<String>,
        home: impl Into<String>,
        device: impl Into<String>,
    ) -> Self {
        Self {
            vendor: vendor.into(),
            home: home.into(),
            device: device.into(),
        }
    }

    pub fn from_claim(vendor: Uuid, home: Option<Uuid>, device: Uuid) -> Self {
        Self::new(
            vendor.to_string(),
            home.map(|h| h.to_string()).unwrap_or_default(),
            device.to_string(),
        )
    }

    pub fn cmd(&self) -> String {
        topics::cmd(&self.vendor, &self.home, &self.device)
    }

    pub fn ota(&self) -> String {
        topics::ota(&self.vendor, &self.home, &self.device)
    }

    pub fn shadow(&self) -> String {
        topics::shadow(&self.vendor, &self.home, &self.device)
    }

    pub fn event(&self) -> String {
        topics::evt(&self.vendor, &self.home, &self.device)
    }

    pub fn ack(&self) -> String {
        topics::ack(&self.vendor, &self.home, &self.device)
    }

    /// The two lanes a device listens on. One function, so a factory test and the
    /// runtime cannot disagree about it.
    pub fn subscriptions(&self) -> [String; 2] {
        [self.cmd(), self.ota()]
    }
}

/// What arrived. Everything a device has to react to is one of these; a topic it did
/// not subscribe to cannot produce an event.
#[derive(Debug)]
pub enum DeviceEvent {
    /// The broker accepted the connection and the subscriptions are in place.
    Connected,
    Command(CommandMsg),
    /// A firmware offer. Progress reports come back on the same lane and are filtered
    /// out here — a device must never treat its own report as an instruction.
    OtaOffer(OtaOffer),
    /// The link dropped. Emitted once per failure, not per retry.
    Disconnected(String),
}

pub struct DeviceClient {
    link: Link,
    client: AsyncClient,
    eventloop: rumqttc::EventLoop,
    /// The last-will message, published by the broker if this device vanishes.
    will: Option<ShadowMsg>,
    last_error: Option<String>,
}

impl DeviceClient {
    /// Connect and subscribe. Subscriptions are (re)made on every `ConnAck`, which is
    /// the whole reason this type exists: with a clean session the broker forgets
    /// them, and the device comes back healthy-looking and deaf.
    pub async fn connect(
        link: Link,
        url: &str,
        client_id: &str,
        will: Option<ShadowMsg>,
    ) -> Result<Self> {
        let (host, port) = split_url(url)?;
        let mut opts = MqttOptions::new(client_id, host, port);
        opts.set_keep_alive(Duration::from_secs(30));
        opts.set_clean_session(true);
        // A device presents its own id and secret on the listener that checks credentials; the
        // simulator is a service here, so it uses the service identity from the environment.
        // Nothing else can read the environment this client is built in, and the credential is
        // per-deployment rather than per-device until the claim flow plumbs the device's own.
        if let (Ok(user), Ok(pass)) = (
            std::env::var("HAVEN_MQTT_USER"),
            std::env::var("HAVEN_MQTT_PASS"),
        ) {
            opts.set_credentials(user, pass);
        }

        if let Some(will) = &will {
            opts.set_last_will(rumqttc::LastWill::new(
                link.shadow(),
                serde_json::to_vec(will).context("last will")?,
                QoS::AtLeastOnce,
                true,
            ));
        }

        let (client, eventloop) = AsyncClient::new(opts, 32);
        Ok(Self {
            link,
            client,
            eventloop,
            will,
            last_error: None,
        })
    }

    pub fn link(&self) -> &Link {
        &self.link
    }

    /// Wait for the next thing worth reacting to.
    ///
    /// Never returns `None`: a device that exits is a device a customer has to
    /// power-cycle, so a broker blip is an event, not the end of the program.
    pub async fn next(&mut self) -> DeviceEvent {
        loop {
            match self.eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => {
                    self.last_error = None;
                    for topic in self.link.subscriptions() {
                        if let Err(e) = self.client.subscribe(&topic, QoS::AtLeastOnce).await {
                            return DeviceEvent::Disconnected(format!("subscribe {topic}: {e}"));
                        }
                    }
                    return DeviceEvent::Connected;
                }
                Ok(Event::Incoming(Packet::Publish(p))) => {
                    if p.topic == self.link.ota() {
                        match parse_ota(&p.payload) {
                            Ok(Some(offer)) => return DeviceEvent::OtaOffer(offer),
                            // Our own progress report echoing back on the same lane.
                            Ok(None) => continue,
                            Err(e) => {
                                tracing::warn!(error = %e, "ota payload did not parse");
                                continue;
                            }
                        }
                    }
                    match serde_json::from_slice::<CommandMsg>(&p.payload) {
                        Ok(command) => return DeviceEvent::Command(command),
                        Err(e) => {
                            tracing::warn!(error = %e, "command envelope did not parse");
                            continue;
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    let text = e.to_string();
                    // One event per outage, not one per retry: a flapping broker would
                    // otherwise flood the log with the same sentence.
                    let first = self.last_error.as_deref() != Some(text.as_str());
                    self.last_error = Some(text.clone());
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    if first {
                        return DeviceEvent::Disconnected(text);
                    }
                }
            }
        }
    }

    pub async fn publish_shadow(&self, report: &StateReport, ts: i64) -> Result<()> {
        self.publish(
            &self.link.shadow(),
            serde_json::to_vec(&report.shadow(ts))?,
            true,
        )
        .await
    }

    pub async fn publish_state(&self, report: &StateReport, ts: i64) -> Result<()> {
        let event: EventMsg = report.event(ts);
        self.publish(&self.link.event(), serde_json::to_vec(&event)?, false)
            .await
    }

    pub async fn publish_ack(&self, ack: &AckMsg) -> Result<()> {
        self.publish(&self.link.ack(), serde_json::to_vec(ack)?, false)
            .await
    }

    pub async fn report_ota(&self, report: &OtaReport) -> Result<()> {
        self.publish(&self.link.ota(), serde_json::to_vec(report)?, false)
            .await
    }

    async fn publish(&self, topic: &str, payload: Vec<u8>, retained: bool) -> Result<()> {
        self.client
            .publish(topic, QoS::AtLeastOnce, retained, payload)
            .await
            .with_context(|| format!("publish {topic}"))
    }

    /// Re-publish the last will so a rebooted device's shadow is current immediately.
    pub fn will(&self) -> Option<&ShadowMsg> {
        self.will.as_ref()
    }

    /// A ready-made "offline" will for a device that is about to go away.
    pub fn offline_will(link: &Link, fw: &str) -> ShadowMsg {
        let _ = link;
        ShadowMsg {
            v: PROTOCOL_VERSION,
            ts: kernel::now_s(),
            online: false,
            fw: Some(fw.to_string()),
            rssi: None,
            pts: BTreeMap::<String, DpValue>::new(),
        }
    }
}

/// `Some(offer)` for an instruction, `None` for a progress report on the same lane.
fn parse_ota(payload: &[u8]) -> Result<Option<OtaOffer>, serde_json::Error> {
    let raw: serde_json::Value = serde_json::from_slice(payload)?;
    if raw.get("state").is_some() {
        return Ok(None);
    }
    serde_json::from_value(raw).map(Some)
}

fn split_url(url: &str) -> Result<(String, u16)> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let rest = rest.trim_end_matches('/');
    Ok(match rest.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().context("mqtt port")?),
        None => (rest.to_string(), 1883),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link() -> Link {
        Link::new("v1", "h1", "d1")
    }

    #[test]
    fn every_topic_is_built_from_the_same_link() {
        assert_eq!(link().cmd(), "haven/v1/v1/h1/d1/cmd");
        assert_eq!(link().ack(), "haven/v1/v1/h1/d1/ack");
        assert_eq!(link().ota(), "haven/v1/v1/h1/d1/ota");
        // The device subscribes to exactly two lanes; the ack lane is publish-only.
        let subs = link().subscriptions();
        assert_eq!(subs.len(), 2);
        assert!(subs.contains(&link().cmd()));
        assert!(subs.contains(&link().ota()));
    }

    #[test]
    fn a_link_from_a_claim_matches_one_built_by_hand() {
        let vendor = Uuid::from_u128(1);
        let home = Uuid::from_u128(2);
        let device = Uuid::from_u128(3);
        assert_eq!(
            Link::from_claim(vendor, Some(home), device),
            Link::new(vendor.to_string(), home.to_string(), device.to_string())
        );
    }

    #[test]
    fn an_ota_report_echoing_back_is_not_an_offer() {
        let report = serde_json::json!({
            "v": 1, "ts": 1, "campaign_id": "01a1ce08-0000-7000-8000-000000000000",
            "state": "downloading"
        });
        assert!(
            parse_ota(report.to_string().as_bytes())
                .expect("parse")
                .is_none()
        );

        let offer = serde_json::json!({
            "campaign_id": "01a1ce08-0000-7000-8000-000000000000", "version": "2.0.0"
        });
        assert!(
            parse_ota(offer.to_string().as_bytes())
                .expect("parse")
                .is_some()
        );
    }

    #[test]
    fn urls_split_with_or_without_a_port() {
        assert_eq!(
            split_url("mqtt://broker:1883").expect("url"),
            ("broker".into(), 1883)
        );
        assert_eq!(
            split_url("mqtt://broker").expect("url"),
            ("broker".into(), 1883)
        );
        assert_eq!(
            split_url("mqtts://broker.haven.dev:8883/").expect("url"),
            ("broker.haven.dev".into(), 8883)
        );
    }
}
