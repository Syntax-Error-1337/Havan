//! Every metric the endpoint can emit is documented.
//!
//! The table in `docs/05-API.md` described six of the eleven families this service exposes — the stream fan-out gauges
//! and the undelivered-command counter had never been written down. A metric nobody documented is a metric nobody
//! alerts on, which is the same as not having it: the whole point of `haven_stream_fanout` is that a dashboard has to know
//! which mode the pod is in.
//!
//! So the scrape is compared with the document, in the direction that fails when the code grows and the prose does not.

use api::{AppState, router};

async fn boot() -> Option<String> {
    let db = std::env::var("TEST_DATABASE_URL").ok()?;
    let redis = std::env::var("TEST_REDIS_URL").ok()?;

    let cfg = std::sync::Arc::new(kernel::Config::from_env().ok()?);
    let pg = infra::Pg::connect(&db, 4).await.ok()?;
    let cache = infra::Cache::connect(&redis).await.ok()?;
    let mqtt = infra::Mqtt::connect(
        &std::env::var("MQTT_URL").unwrap_or_else(|_| "mqtt://localhost:1883".into()),
        &format!("haven-api-metrics-test-{}", uuid::Uuid::now_v7()),
    )
    .ok()?;
    let state = AppState::new(cfg, pg, cache, mqtt);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.ok()?;
    let addr = listener.local_addr().ok()?;
    tokio::spawn(async move {
        let _ = axum::serve(
            listener,
            router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
    });
    Some(format!("http://{addr}"))
}

#[tokio::test]
async fn every_metric_the_endpoint_emits_is_in_the_documented_table() {
    let Some(base) = boot().await else {
        eprintln!("skipping: TEST_DATABASE_URL / TEST_REDIS_URL not set");
        return;
    };

    // Loopback is the operator door, which is exactly how a sidecar scrapes it.
    let text = reqwest::Client::new()
        .get(format!("{base}/metrics"))
        .send()
        .await
        .expect("scrape")
        .text()
        .await
        .expect("body");

    let mut families: Vec<String> = text
        .lines()
        .filter_map(|line| line.strip_prefix("# TYPE "))
        .filter_map(|rest| rest.split_whitespace().next())
        .map(str::to_string)
        .collect();
    families.sort();
    families.dedup();

    assert!(
        families.len() >= 8,
        "the scrape found only {} families; it is looking in the wrong place: {families:?}",
        families.len()
    );

    let doc = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/05-API.md"),
    )
    .expect("docs/05-API.md");

    let missing: Vec<&String> = families
        .iter()
        .filter(|name| !doc.contains(name.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "these metrics are emitted but never documented: {missing:?}"
    );
}
