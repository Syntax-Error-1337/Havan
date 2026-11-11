//! `haven-sim` — a virtual device that speaks the real contract.
//!
//! Claim, shadow, events, commands and acks: the same envelopes a vendor's
//! firmware sends. CI and demos run against this, so the protocol is exercised
//! long before a board exists.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::Context;
use protocol::envelope::{AckMsg, CommandMsg, CommandOp, DpValue, EventMsg, ShadowMsg};
use protocol::topics;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use serde_json::json;
use uuid::Uuid;

#[derive(Debug, Clone)]
struct Point {
    dp: String,
    value: DpValue,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let api = env_or("HAVEN_API", "http://127.0.0.1:8080");
    let mqtt_url = env_or("HAVEN_MQTT", "mqtt://127.0.0.1:1883");
    let pk = std::env::var("HAVEN_PK").context("HAVEN_PK is required")?;
    let sn = std::env::var("HAVEN_SN").context("HAVEN_SN is required")?;
    let secret = std::env::var("HAVEN_SECRET").context("HAVEN_SECRET is required")?;
    let ticket = std::env::var("HAVEN_TICKET").context("HAVEN_TICKET is required")?;

    // 1. Claim. This is the only call that uses the factory secret.
    let claim: serde_json::Value = reqwest::Client::new()
        .post(format!("{api}/v1/provision/claim"))
        .json(&json!({ "pk": pk, "sn": sn, "secret": secret, "ticket": ticket, "fw": "1.0.0" }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let device_id: Uuid = claim["device_id"]
        .as_str()
        .context("claim returned no device_id")?
        .parse()?;
    let home_id = claim["home_id"].as_str().unwrap_or_default().to_string();

    tracing::info!(%device_id, %home_id, points = %claim["points"], "claimed");

    // 2. Connect as the device.
    let (host, port) = split_url(&mqtt_url)?;
    let mut opts = MqttOptions::new(device_id.to_string(), host, port);
    opts.set_keep_alive(Duration::from_secs(30));
    opts.set_last_will(rumqttc::LastWill::new(
        topics::shadow("sim", &home_id, &device_id.to_string()),
        serde_json::to_vec(&ShadowMsg {
            v: 1,
            ts: kernel::now_s(),
            online: false,
            fw: Some("1.0.0".into()),
            rssi: None,
            pts: BTreeMap::new(),
        })?,
        QoS::AtLeastOnce,
        true,
    ));

    let (client, mut eventloop) = AsyncClient::new(opts, 32);

    let device = device_id.to_string();
    client
        .subscribe(topics::cmd("sim", &home_id, &device), QoS::AtLeastOnce)
        .await?;

    // The reference bulb's datapoints: 1 = power (bool), 2 = brightness (int),
    // 3 = kelvin (int). A real firmware derives these from the product model.
    let mut points: Vec<Point> = vec![
        Point { dp: "1".into(), value: DpValue::Bool(false) },
        Point { dp: "2".into(), value: DpValue::Int(80) },
        Point { dp: "3".into(), value: DpValue::Int(4000) },
    ];
    let _ = &mut points;

    // 3. Shadow on connect: retained, so a late subscriber sees the truth.
    publish_shadow(&client, &home_id, &device, &points).await?;

    // 4. Serve commands.
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                publish_shadow(&client, &home_id, &device, &points).await?;
            }
            event = eventloop.poll() => {
                match event {
                    Ok(Event::Incoming(Packet::Publish(p))) => {
                        let Ok(cmd) = serde_json::from_slice::<CommandMsg>(&p.payload) else {
                            tracing::warn!("command envelope did not parse");
                            continue;
                        };
                        apply(&mut points, &cmd);
                        publish_event(&client, &home_id, &device, &points).await?;

                        let ack = AckMsg {
                            v: 1,
                            id: cmd.id,
                            ts: kernel::now_s(),
                            ok: true,
                            dp: cmd.dp.clone(),
                            value: points_for(&points).get(cmd.dp.as_deref().unwrap_or("")).cloned(),
                            code: None,
                            msg: None,
                        };
                        client
                            .publish(
                                topics::ack("sim", &home_id, &device),
                                QoS::AtLeastOnce,
                                false,
                                serde_json::to_vec(&ack)?,
                            )
                            .await?;
                        tracing::info!(cmd = %cmd.id, op = ?cmd.op, "acked");
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "mqtt error; retrying");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        }
    }
}

fn apply(points: &mut [Point], cmd: &CommandMsg) {
    if cmd.op == CommandOp::Action {
        return;
    }
    let Some(dp) = cmd.dp.as_deref() else { return };
    if let (Some(p), Some(v)) = (
        points.iter_mut().find(|p| p.dp == dp),
        cmd.value.as_ref(),
    ) {
        p.value = v.clone();
    }
}

fn points_for(points: &[Point]) -> BTreeMap<String, DpValue> {
    points.iter().map(|p| (p.dp.clone(), p.value.clone())).collect()
}

async fn publish_shadow(
    client: &AsyncClient,
    home: &str,
    device: &str,
    points: &[Point],
) -> anyhow::Result<()> {
    let shadow = ShadowMsg {
        v: 1,
        ts: kernel::now_s(),
        online: true,
        fw: Some("1.0.0".into()),
        rssi: Some(-58),
        pts: points_for(points),
    };
    client
        .publish(
            topics::shadow("sim", home, device),
            QoS::AtLeastOnce,
            true,
            serde_json::to_vec(&shadow)?,
        )
        .await?;
    Ok(())
}

async fn publish_event(
    client: &AsyncClient,
    home: &str,
    device: &str,
    points: &[Point],
) -> anyhow::Result<()> {
    let evt = EventMsg {
        v: 1,
        ts: kernel::now_s(),
        seq: None,
        pts: points_for(points),
        batt: None,
        rssi: None,
    };
    client
        .publish(
            topics::evt("sim", home, device),
            QoS::AtLeastOnce,
            false,
            serde_json::to_vec(&evt)?,
        )
        .await?;
    Ok(())
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn split_url(url: &str) -> anyhow::Result<(String, u16)> {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let rest = rest.trim_end_matches('/');
    Ok(match rest.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse()?),
        None => (rest.to_string(), 1883),
    })
}
