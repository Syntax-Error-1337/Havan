//! `haven-worker` — everything that happens without a request: MQTT ingest,
//! automation evaluation, OTA campaigns, retention, notification fan-out.
//!
//! Phase 0: it subscribes to the device lanes, validates that each topic is in
//! contract, and reports what it saw. The writers land slice by slice, each
//! behind [`ingest`] so this binary stays a supervisor.

use anyhow::Context;
use domain::devices::CommandRepo;
use domain::notification::NotificationRepo;
use domain::pairing::PairingRepo;
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

    // Whether the broker is currently connected. `rumqttc::AsyncClient::publish` accepts
    // packets while disconnected and drops them, so redelivery has to ask this, not the
    // client — otherwise it would mark commands delivered that never left the process.
    let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));

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
        let commands = commands.clone();
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

    // Redelivery: a command refused during a broker outage is not lost, it is late.
    // Bounded in both directions — only the last two minutes, and only while the broker
    // is actually back — because a command delivered ten minutes late turns a light on
    // for no reason, and a redelivery loop that hammers a dead broker is its own outage.
    {
        let commands = commands.clone();
        let client = client.clone();
        let connected = connected.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            loop {
                tick.tick().await;
                if !connected.load(Ordering::Relaxed) {
                    continue;
                }
                match commands.due_for_redelivery(120, 20).await {
                    Ok(due) if due.is_empty() => {}
                    Ok(due) => {
                        for r in due {
                            let Some(value) = r
                                .value
                                .as_ref()
                                .and_then(protocol::envelope::DpValue::from_json)
                            else {
                                // A command whose payload no longer parses cannot be
                                // replayed; leave it for the timeout sweep to settle.
                                continue;
                            };
                            let msg = protocol::envelope::CommandMsg {
                                v: protocol::WIRE_VERSION,
                                id: r.command_id,
                                ts: kernel::now_s(),
                                op: protocol::envelope::CommandOp::Set,
                                dp: r.dp.clone(),
                                value: Some(value),
                                action: None,
                            };
                            let Ok(payload) = serde_json::to_vec(&msg) else {
                                continue;
                            };
                            let topic = topics::cmd(
                                &r.vendor_id.to_string(),
                                &r.home_id.to_string(),
                                &r.device_id.to_string(),
                            );
                            match client
                                .publish(&topic, QoS::AtLeastOnce, false, payload)
                                .await
                            {
                                Ok(()) => match commands.mark_redelivered(r.command_id).await {
                                    Ok(true) => info!(command = %r.command_id, "redelivered"),
                                    Ok(false) => {
                                        info!(command = %r.command_id, "already redelivered")
                                    }
                                    Err(e) => warn!(error = ?e, "could not mark redelivered"),
                                },
                                Err(e) => warn!(error = ?e, "redelivery publish failed"),
                            }
                        }
                    }
                    Err(e) => warn!(error = ?e, "redelivery sweep failed"),
                }
            }
        });
    }

    // Push delivery. The transport is chosen once, at startup, and the choice is logged: a
    // deployment that believes it is pushing while it is only logging is a support ticket.
    {
        let repo = Arc::new(infra::PgNotifications::new(pg.clone()));

        let apns = std::env::var("HAVEN_APNS_KEY_FILE").ok().map(|file| {
            let key = std::fs::read_to_string(&file).unwrap_or_default();
            worker::push::Apns::new(
                std::env::var("HAVEN_APNS_ENDPOINT")
                    .unwrap_or_else(|_| "https://api.sandbox.push.apple.com".into()),
                std::env::var("HAVEN_APNS_TOPIC").unwrap_or_else(|_| "com.haven.app".into()),
                &std::env::var("HAVEN_APNS_KEY_ID").unwrap_or_default(),
                &std::env::var("HAVEN_APNS_TEAM_ID").unwrap_or_default(),
                &key,
            )
        });

        match apns {
            Some(Ok(apns)) => {
                info!("push transport: apns");
                spawn_sender(Arc::new(worker::push::Sender::new(repo, Arc::new(apns))));
            }
            Some(Err(e)) => {
                // Credentials that do not parse are a configuration error worth stopping for:
                // falling back to logging would mean alerts quietly stop being real.
                return Err(e).context("HAVEN_APNS_KEY_FILE is set but unusable");
            }
            None => {
                info!("push transport: log (set HAVEN_APNS_KEY_FILE to deliver for real)");
                spawn_sender(Arc::new(worker::push::Sender::new(
                    repo,
                    Arc::new(worker::push::LogTransport),
                )));
            }
        }
    }

    // Certificate expiry. A device whose certificate lapses stops connecting, and the
    // failure looks like broken hardware — so the warning has to arrive while someone can
    // still act on it. Six hours is often enough for a bucket that changes by day, and cheap
    // enough that a restart does not feel expensive.
    {
        let pairing = infra::PgPairing::new(pg.clone());
        let notifications = infra::PgNotifications::new(pg.clone());
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(6 * 3600));
            loop {
                tick.tick().await;
                let due = match pairing.expiring_certificates(30).await {
                    Ok(due) => due,
                    Err(e) => {
                        warn!(error = ?e, "certificate expiry sweep failed");
                        continue;
                    }
                };

                for cert in due {
                    let Some(bucket) = haven_cert::warn_bucket(cert.days_left) else {
                        continue;
                    };
                    let warning = domain::notification::CertificateWarning {
                        device_id: cert.device_id,
                        home_id: cert.home_id,
                        name: cert.name,
                        serial: cert.serial,
                        days_left: cert.days_left,
                        bucket,
                    };
                    match notifications.notify_certificate_expiry(&warning).await {
                        Ok(0) => {}
                        Ok(told) => info!(
                            device = %cert.device_id,
                            days_left = cert.days_left,
                            told,
                            "certificate expiring"
                        ),
                        Err(e) => warn!(error = ?e, "could not warn about an expiring certificate"),
                    }
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

    // Reports are written in batches: one transaction for up to `Batch::CAPACITY` reports
    // per device, flushed when full or when a quarter-second has passed. The time bound is
    // what keeps a quiet fleet live — a device that reports once must not wait for 250
    // messages that will never arrive.
    let mut batch = ingest::Batch::default();
    let mut flush = tokio::time::interval(Duration::from_millis(250));
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("shutting down");
                // Whatever is buffered is still the truth a device reported; write it
                // before leaving, or a deploy silently loses the last quarter-second.
                batch.flush(&ingest).await;
                break;
            }
            _ = flush.tick() => {
                batch.flush(&ingest).await;
            }
            event = eventloop.poll() => {
                match event {
                    Ok(Event::Incoming(Packet::Disconnect)) => {
                        connected.store(false, Ordering::Relaxed);
                    }
                    Ok(Event::Incoming(Packet::Publish(p))) => {
                        match topics::parse(&p.topic) {
                            Some(topic) => {
                                let n = seen.fetch_add(1, Ordering::Relaxed) + 1;
                                ingest
                                    .on_message_batched(&topic, &p.payload, &mut batch)
                                    .await;
                                if batch.is_full() {
                                    batch.flush(&ingest).await;
                                }
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
                    // clean_session(false) means the broker usually keeps our
                    // subscription across a reconnect — usually. If the broker
                    // loses session state (a recreated container, a session
                    // expiry) the worker keeps a healthy connection and ingests
                    // nothing, with no error anywhere. Subscribing again on
                    // CONNACK costs one idempotent frame.
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        connected.store(true, Ordering::Relaxed);
                        if let Err(e) = client
                            .subscribe(topics::INGEST_SHARED_FILTER, QoS::AtLeastOnce)
                            .await
                        {
                            warn!(error = %e, "resubscribe failed");
                        } else {
                            info!("connected; ingest subscription in place");
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        // A dropped connection is not a delivery. Say so before backing off,
                        // or redelivery keeps publishing into a dead socket.
                        connected.store(false, Ordering::Relaxed);
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

/// One delivery loop, whichever transport was chosen.
fn spawn_sender<P, T>(sender: Arc<worker::push::Sender<P, T>>)
where
    P: domain::notification::PushRepo + Send + Sync + 'static,
    T: worker::push::PushTransport + 'static,
{
    tokio::spawn(async move {
        // Thirty seconds: fast enough that an alert is not stale, slow enough that an empty
        // queue costs nothing.
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        loop {
            tick.tick().await;
            match sender.tick(50).await {
                Ok(sweep) if sweep != worker::push::Sweep::default() => info!(
                    delivered = sweep.delivered,
                    no_tokens = sweep.no_tokens,
                    failed = sweep.failed,
                    pruned = sweep.pruned_tokens,
                    "push sweep"
                ),
                Ok(_) => {}
                Err(e) => warn!(error = ?e, "push sweep failed"),
            }
        }
    });
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
