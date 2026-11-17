//! The stream's fan-out: many sockets, one Redis subscription.
//!
//! The first version gave every websocket its own Redis pub/sub connection — obvious code, and a pod
//! holding five thousand phones asked Redis for five thousand subscriptions and made it fan out five
//! thousand copies of every device event. The test below is the whole point of the rewrite: N sockets on
//! one home must produce **one** subscription, and a released channel must actually disappear.

use api::{AppState, router};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

struct Fixture {
    base: String,
    token: String,
    home_id: Uuid,
    redis: redis::aio::MultiplexedConnection,
}

/// How many Redis subscribers a channel has right now, as Redis itself counts them.
async fn subscribers(redis: &mut redis::aio::MultiplexedConnection, channel: &str) -> i64 {
    let (_channel, count): (String, i64) = redis::cmd("PUBSUB")
        .arg("NUMSUB")
        .arg(channel)
        .query_async(redis)
        .await
        .expect("numsub");
    count
}

async fn boot() -> Option<Fixture> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis_url = std::env::var("TEST_REDIS_URL").ok()?;
    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis_url).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        "haven-api-fanout-test",
    )
    .ok()?;
    let state = AppState::new(cfg, pg, cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    let app = router(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let base = format!("http://{addr}");

    let tokens: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/v1/auth/register"))
        .json(&serde_json::json!({
            "email": format!("fanout-{}@haven.dev", Uuid::now_v7()),
            "password": "fanout-Password-1",
        }))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let token = tokens["access"].as_str()?.to_string();

    let home: serde_json::Value = reqwest::Client::new()
        .post(format!("{base}/v1/homes"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "name": "Fanout flat", "timezone": "UTC" }))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    let home_id: Uuid = home["id"].as_str()?.parse().ok()?;

    let client = redis::Client::open(redis_url).ok()?;
    let redis = client.get_multiplexed_async_connection().await.ok()?;

    Some(Fixture {
        base,
        token,
        home_id,
        redis,
    })
}

async fn open(
    f: &Fixture,
    ws_base: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("{ws_base}/v1/stream"))
        .await
        .expect("connect");

    socket
        .send(Message::Text(
            serde_json::json!({ "t": "auth", "token": f.token, "homes": [f.home_id] })
                .to_string()
                .into(),
        ))
        .await
        .expect("auth frame");

    // "ready" arrives once Redis has acknowledged the subscription, so anything counted after this is
    // real and not a race.
    let ready = socket.next().await.expect("frame").expect("ok");
    assert!(
        ready.to_text().unwrap_or_default().contains("ready"),
        "expected ready, got {ready:?}"
    );
    socket
}

#[tokio::test]
async fn many_sockets_on_one_home_hold_one_redis_subscription() {
    let Some(f) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };
    let ws_base = f.base.replace("http://", "ws://");
    let channel = format!("home:{}", f.home_id);

    let mut redis = f.redis.clone();
    let before = subscribers(&mut redis, &channel).await;

    // Six phones, one home.
    let mut sockets = Vec::new();
    for _ in 0..6 {
        sockets.push(open(&f, &ws_base).await);
    }

    let after = subscribers(&mut redis, &channel).await;
    assert_eq!(
        after - before,
        1,
        "six sockets must add one subscription, not six (before {before}, after {after})"
    );

    // The subscription is live: one publish reaches every socket.
    redis::cmd("PUBLISH")
        .arg(&channel)
        .arg(r#"{"t":"device.online","device_id":"probe"}"#)
        .query_async::<i64>(&mut redis)
        .await
        .expect("publish");

    for socket in sockets.iter_mut() {
        let frame = tokio::time::timeout(std::time::Duration::from_secs(5), socket.next())
            .await
            .expect("a frame within 5s")
            .expect("frame")
            .expect("ok");
        assert!(
            frame.to_text().unwrap_or_default().contains("probe"),
            "every socket sees the event: {frame:?}"
        );
    }

    // Closing them all gives the subscription back.
    for mut socket in sockets {
        let _ = socket.close(None).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let closed = subscribers(&mut redis, &channel).await;
    assert!(
        closed <= before,
        "the channel is released when the last socket leaves (before {before}, after {closed})"
    );
}
