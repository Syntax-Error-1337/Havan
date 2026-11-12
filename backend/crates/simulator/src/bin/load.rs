//! `haven-load` — a synthetic field of devices, for measuring the ingest path.
//!
//! `make bench` measures reads. This measures the other half: many devices reporting at
//! once, through the same topics and the same envelopes a real one uses (`protocol` builds
//! both, so the wire format cannot drift from the firmware contract).
//!
//! One connection publishes for many devices. That is a generator artefact and the only
//! place this is not a real device: a real field has one socket per device, so a broker
//! under this load is doing less work per message than it would in production. The numbers
//! are therefore an upper bound on what this machine's ingest path can take, not a
//! prediction of a deployment.
//!
//! ```text
//! LOAD_DEVICES="v:h:d,v:h:d" LOAD_RATE=500 LOAD_SECONDS=30 cargo run --bin haven-load
//! ```

use std::time::{Duration, Instant};

use haven_sdk::{Link, PointSet, StateReport};
use protocol::envelope::DpValue;
use rumqttc::{AsyncClient, MqttOptions, QoS};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("warn")
        .without_time()
        .init();

    let mqtt_url = std::env::var("LOAD_MQTT").unwrap_or_else(|_| "mqtt://127.0.0.1:1883".into());
    let rate: f64 = std::env::var("LOAD_RATE")
        .unwrap_or_else(|_| "200".into())
        .parse()
        .unwrap_or(200.0);
    let seconds: f64 = std::env::var("LOAD_SECONDS")
        .unwrap_or_else(|_| "20".into())
        .parse()
        .unwrap_or(20.0);

    let devices: Vec<Link> = std::env::var("LOAD_DEVICES")
        .unwrap_or_default()
        .split(',')
        .filter_map(parse_link)
        .collect();

    anyhow::ensure!(!devices.is_empty(), "LOAD_DEVICES is empty");
    anyhow::ensure!(rate > 0.0, "LOAD_RATE must be positive");

    let (host, port) = split_url(&mqtt_url);
    let mut opts = MqttOptions::new(format!("haven-load-{}", std::process::id()), host, port);
    opts.set_keep_alive(Duration::from_secs(30));
    // A window, not a queue: once this fills the generator blocks, which is the back
    // pressure a real client would also see. An unbounded queue would measure the
    // generator's memory instead of the platform.
    opts.set_inflight(64);
    opts.set_max_packet_size(64 * 1024, 64 * 1024);
    let (client, mut eventloop) = AsyncClient::new(opts, 1024);

    // The event loop must run for the socket to make progress at all.
    let pump = tokio::spawn(async move {
        loop {
            if let Err(e) = eventloop.poll().await {
                tracing::warn!(error = %e, "mqtt loop error");
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }
    });

    // Two points per report: a bool and a number, which is what a bulb actually sends.
    let points = PointSet::from_pairs([("1", DpValue::Bool(true)), ("2", DpValue::Int(80))]);

    let interval = Duration::from_secs_f64(1.0 / rate);
    let started = Instant::now();
    let deadline = started + Duration::from_secs_f64(seconds);
    let mut published: u64 = 0;
    let mut failed: u64 = 0;
    let mut next = tokio::time::Instant::now();

    while Instant::now() < deadline {
        let link = &devices[(published as usize) % devices.len()];
        let report = StateReport::from_points(&points, "load");
        let event = report.event(kernel::now_s());
        let payload = serde_json::to_vec(&event)?;

        match client
            .publish(&link.event(), QoS::AtLeastOnce, false, payload)
            .await
        {
            Ok(()) => published += 1,
            Err(_) => failed += 1,
        }

        next += interval;
        tokio::time::sleep_until(next).await;
    }

    // Give the inflight window a moment to be acknowledged before reporting. This is not
    // proof of delivery — the broker accepts and queues — so the row count afterwards is
    // the authority, and the caller waits on that.
    tokio::time::sleep(Duration::from_secs(3)).await;

    let elapsed = started.elapsed().as_secs_f64();
    println!(
        "published={published} failed={failed} elapsed={elapsed:.2}s achieved={:.0}/s devices={}",
        published as f64 / elapsed,
        devices.len()
    );

    pump.abort();
    Ok(())
}

/// `vendor:home:device` → the three ids a topic is made of.
fn parse_link(spec: &str) -> Option<Link> {
    let mut parts = spec.trim().split(':');
    let vendor = parts.next()?.parse().ok()?;
    let home = parts.next()?.parse().ok()?;
    let device = parts.next()?.parse().ok()?;
    Some(Link::from_claim(vendor, Some(home), device))
}

fn split_url(url: &str) -> (String, u16) {
    let rest = url
        .trim_start_matches("mqtt://")
        .trim_start_matches("mqtts://");
    match rest.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().unwrap_or(1883)),
        None => (rest.to_string(), 1883),
    }
}
