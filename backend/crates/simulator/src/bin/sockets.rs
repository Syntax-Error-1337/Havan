//! `haven-sockets` — hold N websockets open against one or more API replicas.
//!
//! The fan-out work (`api/src/hub.rs`) made one Redis subscription serve every socket on a channel. Two
//! things it claims were still unproven at scale:
//!
//! * a single pod can hold a thousand sockets on one home without Redis holding a thousand subscriptions;
//! * with more than one replica, a frame published once reaches every socket, wherever it landed — and each
//!   pod holds its own single subscription, which is the cost of the design and should be stated, not assumed.
//!
//! This exists so both are measured rather than reasoned about. It is a load tool, not a test: the assertions
//! live in `api/tests/stream_fanout.rs`, which has to stay runnable in CI.
//!
//! ```text
//! haven-sockets --token <jwt> --home <uuid> --count 1000 --urls ws://127.0.0.1:8080
//! haven-sockets --token <jwt> --home <uuid> --count 1000 --urls ws://127.0.0.1:8080,ws://127.0.0.1:8090
//! ```
//!
//! Prints: sockets opened, how many got "ready", how long the last one took, and — once every socket is
//! armed — how many received the frame published during the run.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use anyhow::Context;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut token = None;
    let mut home = None;
    let mut count = 100usize;
    let mut urls = vec!["ws://127.0.0.1:8080".to_string()];
    let mut hold = 15u64;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--token" => {
                token = args.get(i + 1).cloned();
                i += 2;
            }
            "--home" => {
                home = args.get(i + 1).cloned();
                i += 2;
            }
            "--count" => {
                count = args
                    .get(i + 1)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(count);
                i += 2;
            }
            "--urls" => {
                urls = args
                    .get(i + 1)
                    .map(|v| v.split(',').map(str::to_string).collect())
                    .unwrap_or(urls);
                i += 2;
            }
            "--hold" => {
                hold = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(hold);
                i += 2;
            }
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }

    let token = token.context("--token is required")?;
    let home = home.context("--home is required")?;

    let armed = Arc::new(AtomicUsize::new(0));
    let received = Arc::new(AtomicUsize::new(0));
    let started = Instant::now();

    let mut handles = Vec::with_capacity(count);
    for n in 0..count {
        let url = urls[n % urls.len()].clone();
        let token = token.clone();
        let home = home.clone();
        let armed = armed.clone();
        let received = received.clone();

        handles.push(tokio::spawn(async move {
            let (mut socket, _) = tokio_tungstenite::connect_async(format!("{url}/v1/stream"))
                .await
                .context("connect")?;
            socket
                .send(Message::Text(
                    serde_json::json!({ "t": "auth", "token": token, "homes": [home] })
                        .to_string()
                        .into(),
                ))
                .await
                .context("auth frame")?;

            // Hold the connection open and count frames; this is a fan-out proof, not a reply store.
            loop {
                match socket.next().await {
                    Some(Ok(Message::Text(text))) => {
                        if text.contains("ready") {
                            armed.fetch_add(1, Ordering::Relaxed);
                        } else if text.contains("probe") {
                            received.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(anyhow::anyhow!("socket: {e}")),
                    None => return Ok(()),
                }
            }
        }));
    }

    // Every socket armed: how long the last one took.
    let deadline = Instant::now() + Duration::from_secs(60);
    while armed.load(Ordering::Relaxed) < count && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let armed_at = started.elapsed();
    println!(
        "opened {} sockets across {} replica(s); {} said ready in {:.2}s",
        count,
        urls.len(),
        armed.load(Ordering::Relaxed),
        armed_at.as_secs_f64()
    );

    // Give an external publisher a moment to fire at this channel, then report what arrived.
    tokio::time::sleep(Duration::from_secs(hold)).await;
    println!(
        "{} of {} received the probe frame",
        received.load(Ordering::Relaxed),
        count
    );

    for handle in handles {
        handle.abort();
    }
    Ok(())
}
