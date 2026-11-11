//! `haven-worker` — everything that happens without a request: MQTT ingest,
//! automation evaluation, OTA campaigns, retention, notification fan-out.
//!
//! Phase 0: it subscribes to the device lanes, validates that each topic is in
//! contract, and reports what it saw. The writers land slice by slice, each
//! behind [`ingest`] so this binary stays a supervisor.

use anyhow::Context;
use domain::devices::CommandRepo;
use protocol::topics;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tracing::{info, warn};
use worker::{automation, ingest, ota};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = Arc::new(kernel::Config::from_env().context("configuration")?);
    kernel::telemetry::init(&cfg.env);

    info!(target: "haven::worker", mqtt = %cfg.mqtt_url, "starting");

    let pg = infra::Pg::connect(&cfg.database_url, cfg.db_max_connections)
        .await
        .context("postgres")?;
    let cache = infra::Cache::connect(&cfg.redis_url)
        .await
        .context("redis")?;
    let commands = infra::PgDevices::new(pg.clone());

    // MQTT before anything that publishes: the runner and the sweeper both hold
    // this one client, and one event loop drains it.
    let (host, port) = split_url(&cfg.mqtt_url)?;
    let mut opts = MqttOptions::new(format!("haven-worker-{}", std::process::id()), host, port);
    opts.set_keep_alive(Duration::from_secs(30));
    opts.set_inflight(16);
    opts.set_clean_session(false);
    let (client, mut eventloop) = AsyncClient::new(opts, 64);

    let runner = std::sync::Arc::new(automation::Runner::new(
        infra::PgAutomation::new(pg.clone()),
        client.clone(),
        format!("worker-{}", std::process::id()),
    ));

    let ota_repo = infra::PgOta::new(pg.clone());

    let ingest = std::sync::Arc::new(ingest::Ingest::new(
        infra::PgIngest::new(pg.clone()),
        commands.clone(),
        cache.clone(),
        runner.clone(),
        ota_repo.clone(),
    ));

    // The sweeper is the other half of a command's life: a device that never
    // answers must still reach a settled state, or the app spins forever.
    {
        let ingest = ingest.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            loop {
                tick.tick().await;
                match commands.sweep_timeouts(10).await {
                    Ok(settled) => {
                        for s in settled {
                            tracing::info!(command = %s.command_id, "timed out");
                            ingest.publish_settled(&s).await;
                        }
                    }
                    Err(e) => tracing::warn!(error = ?e, "timeout sweep failed"),
                }
            }
        });
    }

    // OTA: hand a queued job to the device that has to apply it.
    {
        let dispatcher = ota::Dispatcher::new(ota_repo.clone(), client.clone());
        dispatcher.spawn(Duration::from_secs(5));
    }

    // Scheduled automations: lease what is due, run it, reschedule.
    {
        let runner = runner.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            loop {
                tick.tick().await;
                runner.tick().await;
            }
        });
    }

    // One shared subscription: the broker delivers each device message to exactly one
    // worker, which is what makes running three replicas safe.
    client
        .subscribe(topics::INGEST_SHARED_FILTER, QoS::AtLeastOnce)
        .await
        .context("subscribe")?;

    let seen = AtomicU64::new(0);
    let rejected = AtomicU64::new(0);

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("shutting down");
                break;
            }
            event = eventloop.poll() => {
                match event {
                    Ok(Event::Incoming(Packet::Publish(p))) => {
                        match topics::parse(&p.topic) {
                            Some(topic) => {
                                let n = seen.fetch_add(1, Ordering::Relaxed) + 1;
                                ingest.on_message(&topic, &p.payload).await;
                                if n.is_multiple_of(1000) {
                                    info!(seen = n, "ingested");
                                }
                            }
                            None => {
                                let n = rejected.fetch_add(1, Ordering::Relaxed) + 1;
                                warn!(topic = %p.topic, rejected = n, "topic outside the contract");
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        warn!(error = %e, "mqtt loop error; reconnecting");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        }
    }

    info!(
        seen = seen.load(Ordering::Relaxed),
        rejected = rejected.load(Ordering::Relaxed),
        "stopped"
    );
    Ok(())
}

fn split_url(url: &str) -> anyhow::Result<(String, u16)> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| anyhow::anyhow!("MQTT_URL must look like mqtt://host:1883"))?;
    let default_port = match scheme {
        "mqtt" => 1883,
        "mqtts" | "ssl" => 8883,
        other => anyhow::bail!("unsupported MQTT scheme '{other}'"),
    };
    let rest = rest.trim_end_matches('/');
    Ok(match rest.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse()?),
        None => (rest.to_string(), default_port),
    })
}

#[cfg(test)]
mod tests {
    use super::split_url;

    #[test]
    fn splits_urls() {
        assert_eq!(
            split_url("mqtt://localhost:1883").unwrap(),
            ("localhost".into(), 1883)
        );
        assert_eq!(split_url("mqtts://h").unwrap(), ("h".into(), 8883));
        assert!(split_url("nope").is_err());
    }
}
